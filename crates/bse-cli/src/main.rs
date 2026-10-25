use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use bse_api::ServeOptions;
use bse_classify::{
    Classifier, ClassifyOptions, FallbackClassifier, HeuristicClassifier, LlmClassifier,
};
use bse_config::{config_path, expand_tilde, write_sample, Config};
use bse_core::{Gate, GateCtx, GateDecision, Lang, Provider, SearchReport};
use bse_fetch::FetchCache;
use bse_fetch::{FetchError, FetchOptions, FetchedDocument, Fetcher};
use bse_guard::audit::subject_query;
use bse_guard::{sha256_prefixed, AuditLog, CitationGate, CitationStatus, RulesGate};
use bse_index::{DocumentIndex, IndexDocument};
use bse_llm::{LlmClient, LlmProvider, LlmSettings};
use bse_pipeline::{
    domain_of, EvidenceOptions, EvidencePipeline, EvidencePipelineError, EvidenceReport,
    SearchEngine, SearchOptions,
};
use bse_providers::{BraveProvider, DemoProvider, SearxngProvider};
use bse_store::{ClaimDetail, ClaimSummary, LedgerStats, RunStore, Store};
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "bald",
    version,
    about = "BALD — wyszukiwarka twierdzeń, narracji i dowodów (faza 3)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Wyszukaj twierdzenia / narracje.
    Search(SearchArgs),
    /// Sprawdź zapytanie deterministycznym person guardem.
    Guard {
        #[command(subcommand)]
        command: GuardCommand,
    },
    /// Weryfikuj łańcuch audit logu.
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    /// Sprawdź, czy cytat rzeczywiście występuje w źródle.
    Citation {
        #[command(subcommand)]
        command: CitationCommand,
    },
    /// Lista providerów i ich konfiguracji.
    Providers {
        #[command(subcommand)]
        command: ProvidersCommand,
    },
    /// Pobierz stronę przez politykę SSRF (bez indeksowania).
    Fetch(FetchArgs),
    /// Lokalny indeks Tantivy: dodawanie, wyszukiwanie, statystyki.
    Index {
        #[command(subcommand)]
        command: IndexCommand,
    },
    /// Evidence ledger: ekstrakcja twierdzeń, stance i cytatów.
    Evidence {
        #[command(subcommand)]
        command: EvidenceCommand,
    },
    /// Uruchom lokalne UI web na 127.0.0.1 (bez Tauri/GTK).
    Serve(ServeArgs),
    /// Konfiguracja lokalna.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Args)]
struct SearchArgs {
    /// Zapytanie o temat/twierdzenie (nie o osobę).
    query: String,
    /// Użyj offline fixtures zamiast sieci.
    #[arg(long)]
    demo: bool,
    /// Maksymalna liczba wyników.
    #[arg(long, default_value_t = 10)]
    limit: usize,
    /// Nie zapisuj przebiegu w SQLite.
    #[arg(long)]
    no_store: bool,
    /// Język zapytania.
    #[arg(long, value_enum, default_value_t = LangArg::Auto)]
    lang: LangArg,
    /// Wynik jako JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LangArg {
    Auto,
    Pl,
    En,
}

impl From<LangArg> for Lang {
    fn from(value: LangArg) -> Self {
        match value {
            LangArg::Auto => Lang::Auto,
            LangArg::Pl => Lang::Pl,
            LangArg::En => Lang::En,
        }
    }
}

#[derive(Debug, Subcommand)]
enum GuardCommand {
    /// Sprawdź pojedyncze zapytanie.
    Check {
        query: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum AuditCommand {
    /// Sprawdź hash chain.
    Verify {
        /// Domyślnie `security.audit_log` z konfiguracji.
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum CitationCommand {
    /// Porównaj cytat z plikiem źródłowym.
    Check {
        /// Twierdzenie, którego dotyczy cytat (hash trafia do audytu).
        #[arg(long)]
        claim: String,
        /// Fragment, który ma się znaleźć w źródle.
        #[arg(long)]
        quote: String,
        /// Plik z tekstem źródłowym (UTF-8).
        #[arg(long)]
        source_file: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ProvidersCommand {
    /// Pokaż, co jest włączone i czego brakuje.
    List {
        /// Pokaż też providera demo.
        #[arg(long)]
        demo: bool,
    },
}

#[derive(Debug, Args)]
struct FetchArgs {
    url: String,
    /// Nie używaj lokalnego cache.
    #[arg(long)]
    no_cache: bool,
    /// Pokaż cały wyekstrahowany tekst, nie tylko początek.
    #[arg(long)]
    text: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ServeArgs {
    /// Adres loopback, np. 127.0.0.1:7333.
    #[arg(long, default_value = "127.0.0.1:7333")]
    bind: String,
    /// Katalog z zbudowanym UI (dev). Bez tej flagi UI jest wbudowane w binarkę.
    #[arg(long)]
    ui_dir: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum IndexCommand {
    /// Pobierz URL, wyekstrahuj treść i dodaj/zaktualizuj dokument w indeksie.
    Add {
        url: String,
        #[arg(long)]
        no_cache: bool,
        #[arg(long)]
        json: bool,
    },
    /// Szukaj w lokalnym indeksie (guard działa też tutaj).
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Liczba dokumentów w indeksie.
    Stats {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum EvidenceCommand {
    /// Pobierz źródło, wyekstrahuj twierdzenia i zapisz tylko zweryfikowane cytaty.
    Add {
        url: String,
        #[arg(long)]
        no_cache: bool,
        /// Wymuś deterministyczny klasyfikator regułowy (bez Ollama/API).
        #[arg(long)]
        offline: bool,
        #[arg(long, value_enum, default_value_t = LangArg::Auto)]
        lang: LangArg,
        #[arg(long)]
        json: bool,
    },
    /// Lista twierdzeń w ledgerze.
    List {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Pokaż twierdzenie, jego źródła, stance i encje.
    Show {
        /// Pełny claim_id albo jednoznaczny prefiks.
        claim_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Statystyki evidence ledger.
    Stats {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Zapisz przykładowy plik konfiguracji.
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Pokaż ścieżkę pliku konfiguracji.
    Path,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Search(args) => search(args).await,
        Command::Guard { command } => guard(command),
        Command::Audit { command } => audit(command),
        Command::Citation { command } => citation(command),
        Command::Providers { command } => providers(command),
        Command::Fetch(args) => fetch(args).await,
        Command::Index { command } => index(command).await,
        Command::Evidence { command } => evidence(command).await,
        Command::Serve(args) => serve(args).await,
        Command::Config { command } => config(command),
    }
}

async fn serve(args: ServeArgs) -> Result<ExitCode> {
    let bind: SocketAddr = args
        .bind
        .parse()
        .with_context(|| format!("invalid --bind address: {}", args.bind))?;
    if !bind.ip().is_loopback() {
        bail!("`bald serve` może bindować wyłącznie loopback, np. 127.0.0.1:7333");
    }
    let ui_dir = match args.ui_dir {
        Some(path) => {
            if !path.is_dir() {
                bail!("--ui-dir nie jest katalogiem: {}", path.display());
            }
            Some(path)
        }
        None => None,
    };
    let token = std::env::var("BALD_UI_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    bse_api::serve(ServeOptions {
        bind,
        ui_dir,
        token,
    })
    .await
    .context("local UI server failed")?;
    Ok(ExitCode::SUCCESS)
}

async fn search(args: SearchArgs) -> Result<ExitCode> {
    let cfg = Config::load().context("cannot load config")?;
    let audit_path = expand_tilde(&cfg.security.audit_log);
    let audit = AuditLog::open(audit_path).context("cannot open audit log")?;

    let store: Option<Arc<dyn RunStore>> = if args.no_store {
        None
    } else {
        Some(Arc::new(
            Store::open(expand_tilde(&cfg.storage.path)).context("cannot open local store")?,
        ))
    };

    let providers = build_providers(&cfg, args.demo)?;
    if providers.is_empty() {
        bail!(
            "no providers enabled — użyj `bald search \"...\" --demo` albo włącz SearXNG/Brave w {}",
            config_path().display()
        );
    }

    let engine = SearchEngine::new(providers, audit, store);
    let opts = SearchOptions {
        limit: args.limit,
        store: !args.no_store,
    };
    let report = engine
        .search(&args.query, args.lang.into(), &opts)
        .await
        .context("search failed")?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(&report);
    }

    if report.gate.decision == GateDecision::Allow {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(2))
    }
}

fn build_providers(cfg: &Config, demo: bool) -> Result<Vec<Arc<dyn Provider>>> {
    let mut providers: Vec<Arc<dyn Provider>> = Vec::new();

    if demo {
        providers.push(Arc::new(DemoProvider::new()));
    }

    if cfg.providers.searxng.enabled {
        match SearxngProvider::new(
            cfg.searxng_base_url(),
            cfg.providers.searxng.language.clone(),
            cfg.providers.searxng.safesearch,
        ) {
            Ok(provider) => providers.push(Arc::new(provider)),
            Err(err) => eprintln!("warning: SearXNG disabled: {err}"),
        }
    }

    if cfg.providers.brave.enabled {
        match BraveProvider::new(
            cfg.providers.brave.api_key.clone(),
            cfg.providers.brave.country.clone(),
        ) {
            Ok(provider) => providers.push(Arc::new(provider)),
            Err(err) => eprintln!("warning: Brave disabled: {err}"),
        }
    }

    Ok(providers)
}

fn print_report(report: &SearchReport) {
    println!(
        "BALD search · {} · {} ms",
        lang_label(report.lang),
        report.latency_ms
    );
    println!("query: {}", report.query);
    println!(
        "gate: {} ({}) score={:.2}",
        decision_label(report.gate.decision),
        report.gate.gate,
        report.gate.score
    );
    if !report.gate.matched.is_empty() {
        println!("matched: {}", report.gate.matched.join(", "));
    }
    for reason in &report.gate.reasons {
        println!("reason: {reason}");
    }

    if !report.providers.is_empty() {
        println!("providers:");
        for provider in &report.providers {
            match &provider.error {
                Some(error) => println!(
                    "  - {} ERROR {} ms: {}",
                    provider.provider, provider.latency_ms, error
                ),
                None => println!(
                    "  - {} {} hits / {} ms",
                    provider.provider, provider.hits, provider.latency_ms
                ),
            }
        }
    }

    println!(
        "diversity: {:.2} ({} domen), near-dupes: {}",
        report.source_diversity, report.unique_domains, report.near_duplicates_removed
    );

    for warning in &report.warnings {
        println!("warning: {warning}");
    }

    if report.gate.decision != GateDecision::Allow {
        println!();
        println!("Zapytanie zatrzymane. BALD szuka tematów i twierdzeń, nie danych o osobach.");
        println!("Spróbuj opisać narrację, np. „teoria o ...”, a nie szukać konkretnej osoby.");
        return;
    }

    if report.hits.is_empty() {
        println!();
        println!("Brak wyników.");
        return;
    }

    println!();
    for (idx, hit) in report.hits.iter().enumerate() {
        println!("{}. [{:.4}] {}", idx + 1, hit.score, hit.title);
        println!("   {}", hit.url);
        if !hit.snippet.is_empty() {
            println!("   {}", hit.snippet);
        }
        let providers = hit.providers.join(", ");
        let published = hit
            .published
            .as_deref()
            .map(|p| format!(" · {p}"))
            .unwrap_or_default();
        println!("   {providers}{published}");
    }

    if let Some(run_id) = &report.stored_run_id {
        println!();
        println!("run: {run_id}");
    }
}

fn decision_label(decision: GateDecision) -> &'static str {
    match decision {
        GateDecision::Allow => "ALLOW",
        GateDecision::Rewrite => "REWRITE",
        GateDecision::Review => "REVIEW",
        GateDecision::Refuse => "REFUSED",
    }
}

fn lang_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Auto => "auto",
        Lang::Pl => "pl",
        Lang::En => "en",
    }
}

fn guard(command: GuardCommand) -> Result<ExitCode> {
    match command {
        GuardCommand::Check { query, json } => {
            let verdict = RulesGate::new().check(&GateCtx::search(&query, Lang::Auto));
            if json {
                println!("{}", serde_json::to_string_pretty(&verdict)?);
            } else {
                println!(
                    "{} ({}) score={:.2}",
                    decision_label(verdict.decision),
                    verdict.gate,
                    verdict.score
                );
                if !verdict.matched.is_empty() {
                    println!("matched: {}", verdict.matched.join(", "));
                }
                for reason in &verdict.reasons {
                    println!("reason: {reason}");
                }
            }
            Ok(if verdict.decision == GateDecision::Allow {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            })
        }
    }
}

fn audit(command: AuditCommand) -> Result<ExitCode> {
    match command {
        AuditCommand::Verify { path } => {
            let cfg = Config::load().context("cannot load config")?;
            let path = path.unwrap_or_else(|| expand_tilde(&cfg.security.audit_log));
            let chain = bse_guard::audit::verify_file(&path)
                .with_context(|| format!("cannot verify {}", path.display()))?;
            if chain.valid {
                println!("audit: OK ({} records) — {}", chain.records, path.display());
                Ok(ExitCode::SUCCESS)
            } else {
                println!(
                    "audit: INVALID at line {:?}: {}",
                    chain.first_bad,
                    chain.error.unwrap_or_default()
                );
                Ok(ExitCode::FAILURE)
            }
        }
    }
}

fn citation(command: CitationCommand) -> Result<ExitCode> {
    let CitationCommand::Check {
        claim,
        quote,
        source_file,
        json,
    } = command;

    let source = std::fs::read_to_string(&source_file)
        .with_context(|| format!("cannot read source {}", source_file.display()))?;
    let verdict = CitationGate::new().verify(&claim, &quote, &source);
    let verified = verdict.is_verified();
    let status = citation_status_label(verdict.status);

    let cfg = Config::load().context("cannot load config")?;
    let audit_path = expand_tilde(&cfg.security.audit_log);
    let audit_seq = match AuditLog::open(audit_path) {
        Ok(mut log) => {
            let detail = serde_json::json!({
                "status": status,
                "score": verdict.score,
                "quote_hash": &verdict.quote_hash,
                "source_hash": &verdict.source_hash,
                "claim_hash": &verdict.claim_hash,
                "span": verdict.span,
            });
            match log.append(
                "citation.check",
                &verdict.quote_hash,
                "citation",
                status,
                detail,
            ) {
                Ok(record) => Some(record.seq),
                Err(err) => {
                    eprintln!("warning: audit: {err}");
                    None
                }
            }
        }
        Err(err) => {
            eprintln!("warning: audit: {err}");
            None
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&verdict)?);
    } else {
        println!("citation: {status} score={:.3}", verdict.score);
        println!("claim:  {}", verdict.claim_hash);
        println!("quote:  {}", verdict.quote_hash);
        println!("source: {}", verdict.source_hash);
        if let Some(span) = verdict.span {
            println!("span:   {}..{} (normalized)", span.start, span.end);
        }
        if let Some(seq) = audit_seq {
            println!("audit:  seq {seq}");
        }
        if !verified {
            println!();
            println!("Cytat odrzucony — zdanie nie może wejść do syntezy bez pokrycia w źródle.");
        }
    }

    Ok(if verified {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

fn citation_status_label(status: CitationStatus) -> &'static str {
    match status {
        CitationStatus::VerifiedExact => "verified_exact",
        CitationStatus::VerifiedNormalized => "verified_normalized",
        CitationStatus::Rejected => "rejected",
        CitationStatus::SourceTooShort => "source_too_short",
    }
}

fn providers(command: ProvidersCommand) -> Result<ExitCode> {
    match command {
        ProvidersCommand::List { demo } => {
            let cfg = Config::load().context("cannot load config")?;
            println!(
                "{:<10} {:<8} {}",
                "demo",
                if demo { "enabled" } else { "disabled" },
                if demo {
                    "offline fixtures (6 hits)"
                } else {
                    "użyj --demo"
                }
            );
            println!(
                "{:<10} {:<8} {}",
                "searxng",
                enabled(cfg.providers.searxng.enabled),
                cfg.searxng_base_url()
            );
            println!(
                "{:<10} {:<8} key={}",
                "brave",
                enabled(cfg.providers.brave.enabled),
                match &cfg.providers.brave.api_key {
                    Some(k) if !k.trim().is_empty() => "set",
                    _ => "missing",
                }
            );
            println!(
                "{:<10} {:<8} {} · model={} · key={}",
                "llm",
                cfg.llm.provider,
                cfg.llm.base_url,
                cfg.llm.model,
                if cfg.llm.api_key().is_some() {
                    "set"
                } else {
                    "not-needed/missing"
                }
            );
            println!(
                "{:<10} {:<8} {} · model={}",
                "fallback",
                cfg.llm.fallback_provider,
                cfg.llm.fallback_base_url,
                cfg.llm.fallback_model
            );
            println!();
            println!("config: {}", config_path().display());
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn enabled(value: bool) -> &'static str {
    if value {
        "enabled"
    } else {
        "disabled"
    }
}

fn build_fetcher(cfg: &Config, no_cache: bool) -> Result<Fetcher> {
    let options = FetchOptions {
        max_bytes: cfg.fetch.max_bytes.max(1),
        max_redirects: cfg.fetch.max_redirects,
        timeout: Duration::from_secs(cfg.fetch.timeout_secs.max(1)),
        respect_robots: cfg.fetch.respect_robots,
        ..FetchOptions::default()
    };
    let fetcher = Fetcher::new(options).context("cannot build HTTP fetcher")?;
    if no_cache {
        return Ok(fetcher);
    }

    let cache = FetchCache::new(
        expand_tilde(&cfg.fetch.cache_dir),
        Duration::from_secs(cfg.fetch.cache_ttl_secs),
    )
    .context("cannot open fetch cache")?;
    Ok(fetcher.with_cache(cache))
}

async fn fetch_audited(
    fetcher: &Fetcher,
    raw_url: &str,
    audit: &mut AuditLog,
) -> Result<FetchedDocument, FetchError> {
    let subject = sha256_prefixed(raw_url);
    match fetcher.fetch(raw_url).await {
        Ok(document) => {
            if let Err(error) = audit.append(
                "fetch.request",
                &subject,
                "ssrf",
                "allow",
                serde_json::json!({
                    "status": document.status,
                    "content_hash": &document.content_hash,
                    "text_chars": document.text.chars().count(),
                    "cache_hit": document.cache_hit,
                    "noindex": document.noindex,
                }),
            ) {
                eprintln!("warning: audit: {error}");
            }
            Ok(document)
        }
        Err(error) => {
            if let Err(audit_error) = audit.append(
                "fetch.request",
                &subject,
                "ssrf",
                "refuse",
                serde_json::json!({ "error": error.kind() }),
            ) {
                eprintln!("warning: audit: {audit_error}");
            }
            Err(error)
        }
    }
}

async fn fetch(args: FetchArgs) -> Result<ExitCode> {
    let cfg = Config::load().context("cannot load config")?;
    let mut audit =
        AuditLog::open(expand_tilde(&cfg.security.audit_log)).context("cannot open audit log")?;
    let fetcher = build_fetcher(&cfg, args.no_cache)?;

    let document = match fetch_audited(&fetcher, &args.url, &mut audit).await {
        Ok(document) => document,
        Err(error) => {
            eprintln!("fetch: {error}");
            return Ok(ExitCode::from(2));
        }
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&document)?);
    } else {
        print_fetched(&document, args.text);
    }
    Ok(ExitCode::SUCCESS)
}

fn print_fetched(document: &FetchedDocument, full_text: bool) {
    println!("status:    {}", document.status);
    println!("url:       {}", document.final_url);
    if document.requested_url != document.final_url {
        println!("requested: {}", document.requested_url);
    }
    println!("canonical: {}", document.canonical_url);
    println!("title:     {}", document.title);
    if !document.description.is_empty() {
        println!("description: {}", document.description);
    }
    if let Some(lang) = &document.lang {
        println!("lang:      {lang}");
    }
    if let Some(published) = &document.published {
        println!("published: {published}");
    }
    println!("chars:     {}", document.text.chars().count());
    println!("hash:      {}", document.content_hash);
    println!(
        "cache:     {}",
        if document.cache_hit { "hit" } else { "miss" }
    );
    if document.noindex {
        println!("robots:    noindex (nie zostanie zindeksowany)");
    }

    println!();
    if full_text {
        println!("{}", document.text);
    } else {
        let excerpt: String = document.text.chars().take(800).collect();
        println!("{excerpt}");
    }
}

async fn index(command: IndexCommand) -> Result<ExitCode> {
    match command {
        IndexCommand::Add {
            url,
            no_cache,
            json,
        } => index_add(url, no_cache, json).await,
        IndexCommand::Search { query, limit, json } => index_search(query, limit, json),
        IndexCommand::Stats { json } => index_stats(json),
    }
}

fn open_index(cfg: &Config) -> Result<DocumentIndex> {
    DocumentIndex::open(expand_tilde(&cfg.storage.index_path)).context("cannot open local index")
}

async fn index_add(raw_url: String, no_cache: bool, json: bool) -> Result<ExitCode> {
    let cfg = Config::load().context("cannot load config")?;
    let mut audit =
        AuditLog::open(expand_tilde(&cfg.security.audit_log)).context("cannot open audit log")?;
    let fetcher = build_fetcher(&cfg, no_cache)?;

    let document = match fetch_audited(&fetcher, &raw_url, &mut audit).await {
        Ok(document) => document,
        Err(error) => {
            eprintln!("index add: {error}");
            return Ok(ExitCode::from(2));
        }
    };

    if document.noindex {
        let subject = sha256_prefixed(&document.canonical_url);
        if let Err(error) = audit.append(
            "index.add",
            &subject,
            "robots",
            "skip",
            serde_json::json!({ "reason": "noindex" }),
        ) {
            eprintln!("warning: audit: {error}");
        }
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "indexed": false,
                    "reason": "noindex",
                    "url": document.canonical_url,
                }))?
            );
        } else {
            println!("not indexed: {}", document.canonical_url);
            println!("reason: meta robots noindex");
        }
        return Ok(ExitCode::SUCCESS);
    }

    if document.text.trim().is_empty() {
        bail!(
            "no extractable text at {} (JavaScript-rendered page?)",
            document.final_url
        );
    }

    let index = open_index(&cfg)?;
    let domain = domain_of(&document.canonical_url);
    let title = if document.title.trim().is_empty() {
        document.canonical_url.clone()
    } else {
        document.title.clone()
    };
    let index_document = IndexDocument::new(
        document.canonical_url.clone(),
        title,
        document.text.clone(),
        document.description.clone(),
        domain.clone(),
        document.published.clone(),
        document.lang.clone(),
        document.fetched_at.clone(),
        document.content_hash.clone(),
        document.html_hash.clone(),
    );
    index
        .add(&index_document)
        .context("cannot write document to index")?;
    let stats = index.stats().context("cannot read index stats")?;

    let subject = sha256_prefixed(&document.canonical_url);
    if let Err(error) = audit.append(
        "index.add",
        &subject,
        "index",
        "allow",
        serde_json::json!({
            "content_hash": &document.content_hash,
            "chars": document.text.chars().count(),
            "documents": stats.documents,
        }),
    ) {
        eprintln!("warning: audit: {error}");
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "indexed": true,
                "url": document.canonical_url,
                "title": index_document.title,
                "domain": domain,
                "chars": document.text.chars().count(),
                "content_hash": document.content_hash,
                "cache_hit": document.cache_hit,
                "documents": stats.documents,
            }))?
        );
    } else {
        println!("indexed: {}", document.canonical_url);
        println!("title:   {}", index_document.title);
        println!("chars:   {}", document.text.chars().count());
        println!("hash:    {}", document.content_hash);
        println!(
            "cache:   {}",
            if document.cache_hit { "hit" } else { "miss" }
        );
        println!("documents: {}", stats.documents);
    }
    Ok(ExitCode::SUCCESS)
}

fn index_search(query: String, limit: usize, json: bool) -> Result<ExitCode> {
    let query = query.trim().to_string();
    if query.is_empty() {
        bail!("empty query");
    }

    let cfg = Config::load().context("cannot load config")?;
    let mut audit =
        AuditLog::open(expand_tilde(&cfg.security.audit_log)).context("cannot open audit log")?;
    let subject = subject_query(&query);
    let verdict = RulesGate::new().check(&GateCtx::search(&query, Lang::Auto));

    if let Err(error) = audit.append(
        "query.guard",
        &subject,
        &verdict.gate,
        decision_code(verdict.decision),
        serde_json::json!({
            "reasons": &verdict.reasons,
            "matched": &verdict.matched,
            "score": verdict.score,
            "surface": "index",
        }),
    ) {
        eprintln!("warning: audit: {error}");
    }

    if verdict.decision != GateDecision::Allow {
        if json {
            println!("{}", serde_json::to_string_pretty(&verdict)?);
        } else {
            println!(
                "{} ({}) score={:.2}",
                decision_label(verdict.decision),
                verdict.gate,
                verdict.score
            );
            println!("Zapytanie zatrzymane. Szukaj tematu/twierdzenia, nie osoby.");
        }
        return Ok(ExitCode::from(2));
    }

    let started = Instant::now();
    let index = open_index(&cfg)?;
    let hits = index
        .search(&query, limit.clamp(1, 500))
        .context("index search failed")?;
    let latency_ms = started.elapsed().as_millis() as u64;

    if let Err(error) = audit.append(
        "index.search",
        &subject,
        "index",
        "allow",
        serde_json::json!({
            "hits": hits.len(),
            "latency_ms": latency_ms,
        }),
    ) {
        eprintln!("warning: audit: {error}");
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "query": query,
                "hits": hits,
                "latency_ms": latency_ms,
                "index_path": expand_tilde(&cfg.storage.index_path),
            }))?
        );
    } else {
        println!("BALD local index · {} hits · {latency_ms} ms", hits.len());
        println!("query: {query}");
        if hits.is_empty() {
            println!();
            println!("Brak wyników.");
            return Ok(ExitCode::SUCCESS);
        }
        println!();
        for (idx, hit) in hits.iter().enumerate() {
            println!("{}. [{:.4}] {}", idx + 1, hit.score, hit.title);
            println!("   {}", hit.url);
            if !hit.excerpt.is_empty() {
                println!("   {}", hit.excerpt);
            }
            let published = hit
                .published
                .as_deref()
                .map(|p| format!(" · {p}"))
                .unwrap_or_default();
            println!("   {}{published}", hit.domain);
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn index_stats(json: bool) -> Result<ExitCode> {
    let cfg = Config::load().context("cannot load config")?;
    let index = open_index(&cfg)?;
    let stats = index.stats().context("cannot read index stats")?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "documents": stats.documents,
                "path": expand_tilde(&cfg.storage.index_path),
            }))?
        );
    } else {
        println!("documents: {}", stats.documents);
        println!(
            "path:      {}",
            expand_tilde(&cfg.storage.index_path).display()
        );
    }
    Ok(ExitCode::SUCCESS)
}

async fn evidence(command: EvidenceCommand) -> Result<ExitCode> {
    match command {
        EvidenceCommand::Add {
            url,
            no_cache,
            offline,
            lang,
            json,
        } => evidence_add(url, no_cache, offline, lang.into(), json).await,
        EvidenceCommand::List { limit, json } => evidence_list(limit, json),
        EvidenceCommand::Show { claim_id, json } => evidence_show(claim_id, json),
        EvidenceCommand::Stats { json } => evidence_stats(json),
    }
}

fn classify_options(cfg: &Config) -> ClassifyOptions {
    ClassifyOptions {
        max_input_chars: cfg.llm.max_input_chars.max(1_000),
        max_claims: cfg.llm.max_claims.clamp(1, 50),
        max_attempts: cfg.llm.max_attempts.clamp(1, 4),
        min_confidence: 0.0,
    }
}

fn classifier_backend(
    cfg: &Config,
    provider: &str,
    base_url: &str,
    model: &str,
    api_key: Option<String>,
) -> Result<Arc<dyn Classifier>> {
    let provider = provider.trim().to_ascii_lowercase();
    match provider.as_str() {
        "none" | "heuristic" => Ok(Arc::new(HeuristicClassifier::new(classify_options(cfg)))),
        "ollama" | "openai" => {
            let provider = LlmProvider::parse(&provider)
                .ok_or_else(|| anyhow::anyhow!("unknown LLM provider: {provider}"))?;
            let settings = LlmSettings {
                provider,
                base_url: base_url.to_string(),
                model: model.to_string(),
                api_key,
                timeout: Duration::from_secs(cfg.llm.timeout_secs.max(1)),
                connect_timeout: Duration::from_secs(cfg.llm.connect_timeout_secs.max(1)),
                temperature: cfg.llm.temperature,
                max_tokens: cfg.llm.max_tokens.max(64),
            };
            let client = LlmClient::new(settings).context("invalid LLM configuration")?;
            Ok(Arc::new(LlmClassifier::new(
                Arc::new(client),
                classify_options(cfg),
            )))
        }
        other => bail!("unknown LLM provider: {other}"),
    }
}

fn build_classifier(cfg: &Config, offline: bool) -> Result<Arc<dyn Classifier>> {
    if offline {
        return Ok(Arc::new(HeuristicClassifier::new(classify_options(cfg))));
    }

    let primary = classifier_backend(
        cfg,
        &cfg.llm.provider,
        &cfg.llm.base_url,
        &cfg.llm.model,
        cfg.llm.api_key(),
    )?;
    if matches!(
        cfg.llm.provider.trim().to_ascii_lowercase().as_str(),
        "none" | "heuristic"
    ) || cfg
        .llm
        .fallback_provider
        .trim()
        .eq_ignore_ascii_case("none")
        || cfg
            .llm
            .fallback_provider
            .trim()
            .eq_ignore_ascii_case(&cfg.llm.provider)
    {
        return Ok(primary);
    }

    let fallback = classifier_backend(
        cfg,
        &cfg.llm.fallback_provider,
        &cfg.llm.fallback_base_url,
        &cfg.llm.fallback_model,
        cfg.llm.fallback_api_key(),
    )?;
    Ok(Arc::new(FallbackClassifier::new(primary, fallback)))
}

fn open_evidence_store(cfg: &Config) -> Result<Arc<Store>> {
    Ok(Arc::new(
        Store::open(expand_tilde(&cfg.storage.path)).context("cannot open local store")?,
    ))
}

async fn evidence_add(
    raw_url: String,
    no_cache: bool,
    offline: bool,
    lang: Lang,
    json: bool,
) -> Result<ExitCode> {
    let cfg = Config::load().context("cannot load config")?;
    let audit =
        AuditLog::open(expand_tilde(&cfg.security.audit_log)).context("cannot open audit log")?;
    let fetcher = build_fetcher(&cfg, no_cache)?;
    let classifier = build_classifier(&cfg, offline)?;
    let store = open_evidence_store(&cfg)?;
    let pipeline = EvidencePipeline::new(
        fetcher,
        classifier,
        store,
        audit,
        EvidenceOptions::default(),
    );

    let report = match pipeline.ingest(&raw_url, lang).await {
        Ok(report) => report,
        Err(error) => {
            eprintln!("evidence add: {error}");
            return Ok(match error {
                EvidencePipelineError::Fetch(_) => ExitCode::from(2),
                _ => ExitCode::FAILURE,
            });
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_evidence_report(&report);
    }
    Ok(ExitCode::SUCCESS)
}

fn evidence_list(limit: usize, json: bool) -> Result<ExitCode> {
    let cfg = Config::load().context("cannot load config")?;
    let store = open_evidence_store(&cfg)?;
    let claims = store
        .list_claims(limit.clamp(1, 500))
        .context("cannot read evidence ledger")?;
    let stats = store
        .ledger_stats()
        .context("cannot read evidence ledger stats")?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "claims": claims,
                "stats": stats,
            }))?
        );
        return Ok(ExitCode::SUCCESS);
    }

    println!(
        "BALD evidence ledger · {} claims · {} evidence · {} entities",
        stats.claims, stats.evidence, stats.entities
    );
    if claims.is_empty() {
        println!();
        println!("Brak twierdzeń. Użyj: bald evidence add <url>");
        return Ok(ExitCode::SUCCESS);
    }
    println!();
    for (index, claim) in claims.iter().enumerate() {
        print_claim_summary(index + 1, claim);
    }
    Ok(ExitCode::SUCCESS)
}

fn evidence_show(claim_id: String, json: bool) -> Result<ExitCode> {
    let cfg = Config::load().context("cannot load config")?;
    let store = open_evidence_store(&cfg)?;
    let resolved = store
        .resolve_claim_id(claim_id.trim())
        .context("cannot resolve claim id")?
        .ok_or_else(|| anyhow::anyhow!("claim not found: {claim_id}"))?;
    let detail = store
        .claim_detail(&resolved)
        .context("cannot read claim")?
        .ok_or_else(|| anyhow::anyhow!("claim not found: {claim_id}"))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&detail)?);
    } else {
        print_claim_detail(&detail);
    }
    Ok(ExitCode::SUCCESS)
}

fn evidence_stats(json: bool) -> Result<ExitCode> {
    let cfg = Config::load().context("cannot load config")?;
    let store = open_evidence_store(&cfg)?;
    let stats = store
        .ledger_stats()
        .context("cannot read evidence ledger stats")?;
    if json {
        println!("{}", serde_json::to_string_pretty(&stats)?);
    } else {
        print_ledger_stats(&stats);
    }
    Ok(ExitCode::SUCCESS)
}

fn print_evidence_report(report: &EvidenceReport) {
    println!(
        "BALD evidence · {} · {} ms",
        report.domain, report.latency_ms
    );
    println!("source:  {}", report.canonical_url);
    if report.requested_url != report.canonical_url {
        println!("request: {}", report.requested_url);
    }
    if report.title != report.canonical_url {
        println!("title:   {}", report.title);
    }
    if let Some(reason) = &report.skipped {
        println!("skipped: {reason}");
    }
    if !report.model.is_empty() {
        println!(
            "model:   {} (prompt {})",
            report.model, report.prompt_version
        );
    }
    println!(
        "claims:  {} accepted, {} rejected",
        report.accepted.len(),
        report.rejected.len()
    );
    println!("content: {}", report.content_hash);

    for (index, claim) in report.accepted.iter().enumerate() {
        println!();
        println!("[{}] {}", index + 1, claim.text);
        println!(
            "    stance: {} · kind: {} · confidence: {:.2}",
            claim.stance.as_str(),
            claim.kind.as_str(),
            claim.confidence
        );
        println!("    quote: {}", claim.quote);
        println!(
            "    citation: {:?} {:.2} · quote {}",
            claim.citation_status, claim.citation_score, claim.quote_hash
        );
        if !claim.entities.is_empty() {
            let entities = claim
                .entities
                .iter()
                .map(|entity| format!("{} ({})", entity.name, entity.kind.as_str()))
                .collect::<Vec<_>>()
                .join(", ");
            println!("    entities: {entities}");
        }
        println!("    claim_id: {}", claim.claim_id);
    }

    if !report.rejected.is_empty() {
        println!();
        println!("rejected:");
        for claim in &report.rejected {
            println!("  - {} ({})", claim.text, claim.reason);
        }
    }

    for warning in &report.warnings {
        println!("warning: {warning}");
    }

    if report.accepted.is_empty() {
        println!();
        println!("Nic nie zapisano: brak twierdzeń, które przeszły bramkę cytatu.");
    } else {
        println!();
        println!(
            "ledger: document={} claims={} evidence={} entities={}",
            report.document_id.as_deref().unwrap_or("-"),
            report.claims_stored,
            report.evidence_stored,
            report.entities_stored
        );
    }
}

fn print_claim_summary(index: usize, claim: &ClaimSummary) {
    let stances = stance_summary(claim);
    println!(
        "{index}. [{:>2} docs · {:>2} evidence{stances}] {}",
        claim.document_count, claim.evidence_count, claim.text
    );
    println!("    kind: {}", claim.kind.as_str());
    println!("    id:   {}", claim.id);
}

fn stance_summary(claim: &ClaimSummary) -> String {
    if claim.stances.is_empty() {
        return String::new();
    }
    let counts = claim
        .stances
        .iter()
        .map(|item| format!("{}={}", item.stance.as_str(), item.count))
        .collect::<Vec<_>>()
        .join(" ");
    format!(" · {counts}")
}

fn print_claim_detail(detail: &ClaimDetail) {
    print_claim_summary(1, &detail.claim);
    if !detail.entities.is_empty() {
        let entities = detail
            .entities
            .iter()
            .map(|entity| format!("{} ({})", entity.name, entity.kind.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        println!("    entities: {entities}");
    }
    println!(
        "    first: {} · last: {}",
        detail.claim.first_seen, detail.claim.last_seen
    );
    println!();
    println!("sources:");
    for (index, item) in detail.evidence.iter().enumerate() {
        println!(
            "  {}. {} · {} · {:.2}",
            index + 1,
            item.stance.as_str(),
            item.domain,
            item.strength
        );
        println!("     {}", item.canonical_url);
        if !item.title.is_empty() {
            println!("     title: {}", item.title);
        }
        println!("     quote: {}", item.quote);
        println!(
            "     model: {} · prompt: {} · quote_hash: {}",
            item.model_version, item.prompt_version, item.quote_hash
        );
        if let (Some(start), Some(end)) = (item.span_start, item.span_end) {
            println!("     span: {start}..{end}");
        }
    }
}

fn print_ledger_stats(stats: &LedgerStats) {
    println!("claims:   {}", stats.claims);
    println!("evidence: {}", stats.evidence);
    println!("entities: {}", stats.entities);
    println!("sources:  {}", stats.documents_with_evidence);
}

fn decision_code(decision: GateDecision) -> &'static str {
    match decision {
        GateDecision::Allow => "allow",
        GateDecision::Rewrite => "rewrite",
        GateDecision::Review => "review",
        GateDecision::Refuse => "refuse",
    }
}

fn config(command: ConfigCommand) -> Result<ExitCode> {
    match command {
        ConfigCommand::Init { force } => {
            let path = config_path();
            let created = write_sample(&path, force).context("cannot write config")?;
            if created {
                println!("created: {}", path.display());
                println!(
                    "Edytuj konfigurację providerów albo użyj zmiennych: BRAVE_API_KEY, BALD_SEARXNG_URL."
                );
            } else {
                println!("exists: {} (użyj --force, aby nadpisać)", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        ConfigCommand::Path => {
            println!("{}", config_path().display());
            Ok(ExitCode::SUCCESS)
        }
    }
}

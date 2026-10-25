use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use bse_config::{config_path, expand_tilde, write_sample, Config};
use bse_core::{Gate, GateCtx, GateDecision, Lang, Provider, SearchReport};
use bse_fetch::FetchCache;
use bse_fetch::{FetchError, FetchOptions, FetchedDocument, Fetcher};
use bse_guard::audit::subject_query;
use bse_guard::{sha256_prefixed, AuditLog, CitationGate, CitationStatus, RulesGate};
use bse_index::{DocumentIndex, IndexDocument};
use bse_pipeline::{domain_of, SearchEngine, SearchOptions};
use bse_providers::{BraveProvider, DemoProvider, SearxngProvider};
use bse_store::{RunStore, Store};
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "bald",
    version,
    about = "BALD — wyszukiwarka twierdzeń i narracji (faza 0)"
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
        Command::Config { command } => config(command),
    }
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
        .search(&query, limit.max(1))
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

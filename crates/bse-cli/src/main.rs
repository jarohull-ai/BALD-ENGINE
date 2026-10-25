use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use bse_config::{config_path, expand_tilde, write_sample, Config};
use bse_core::{Gate, GateCtx, GateDecision, Lang, Provider, SearchReport};
use bse_guard::{AuditLog, CitationGate, CitationStatus, RulesGate};
use bse_pipeline::{SearchEngine, SearchOptions};
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

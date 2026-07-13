mod graph;
mod plan;
mod render;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

/// tfgraph — gera (ou lê) um plan do OpenTofu/Terraform e abre no navegador
/// um diagrama da infraestrutura provisionada (recursos, nomes, relações e
/// dependências).
#[derive(Parser, Debug)]
#[command(name = "tfgraph", version, about, long_about = None)]
struct Args {
    /// Diretório do projeto OpenTofu/Terraform. O tfgraph roda `<cli> plan`
    /// e `<cli> show -json` sozinho aqui dentro. Padrão: diretório atual.
    /// Ignorado se --plan for informado.
    #[arg(short = 'd', long = "dir")]
    dir: Option<PathBuf>,

    /// Em vez de gerar o plan, usa um arquivo já existente: tanto um JSON
    /// pronto (`<cli> show -json plan.tfplan > plan.json`) quanto o próprio
    /// plan binário (`<cli> show -json` é chamado por baixo dos panos).
    #[arg(short = 'p', long = "plan")]
    plan: Option<PathBuf>,

    /// Qual CLI usar: `tofu` ou `terraform`. Por padrão autodetecta,
    /// preferindo `tofu` se ambos estiverem instalados.
    #[arg(long = "cli")]
    cli: Option<String>,

    /// Argumentos extras repassados para `<cli> plan` (ex: -var-file=prod.tfvars).
    /// Use após `--`, ex: `tfgraph -- -var-file=prod.tfvars`.
    #[arg(last = true)]
    plan_args: Vec<String>,

    /// Onde salvar o HTML gerado. Por padrão usa um arquivo temporário.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Não abre o navegador automaticamente; só gera o HTML.
    #[arg(long)]
    no_open: bool,

    /// Título mostrado no topo da página.
    #[arg(long, default_value = "infraestrutura")]
    title: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let (plan_json, source_label) = if let Some(plan_path) = &args.plan {
        let v = plan::load(plan_path, args.cli.as_deref())?;
        (v, plan_path.display().to_string())
    } else {
        let dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));
        let v = plan::generate(&dir, args.cli.as_deref(), &args.plan_args)?;
        (v, format!("{} (plan gerado agora)", dir.display()))
    };

    let g = graph::build(&plan_json);

    if g.nodes.is_empty() {
        eprintln!(
            "aviso: nenhum recurso encontrado em '{}'. \
             confirme se o plan/JSON está correto.",
            source_label
        );
    } else {
        eprintln!(
            "encontrados {} recursos e {} dependências",
            g.nodes.len(),
            g.edges.len()
        );
    }

    let html = render::render(&g, &args.title, &source_label);

    let output_path = match args.output {
        Some(p) => p,
        None => std::env::temp_dir().join(format!("tfgraph-{}.html", std::process::id())),
    };
    std::fs::write(&output_path, html)
        .with_context(|| format!("falha ao escrever '{}'", output_path.display()))?;

    println!("diagrama gerado em: {}", output_path.display());

    if !args.no_open {
        let url = format!("file://{}", output_path.display());
        if let Err(e) = webbrowser::open(&url) {
            eprintln!("não consegui abrir o navegador automaticamente ({e}); abra manualmente o arquivo acima.");
        }
    }

    Ok(())
}

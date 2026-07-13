use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Stdio};

/// Binários que sabem falar plan em JSON (`<bin> plan` / `<bin> show -json`).
/// OpenTofu é um fork compatível do Terraform e usa exatamente o mesmo
/// schema de JSON, então o resto do programa (graph.rs / render.rs) não
/// precisa saber qual dos dois gerou o plan. `tofu` vem primeiro na lista
/// de autodetecção porque é o motivo desta função existir.
const CANDIDATE_BINARIES: [&str; 2] = ["tofu", "terraform"];

fn binary_exists(bin: &str) -> bool {
    Command::new(bin)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn resolve_binary(forced: Option<&str>) -> Result<String> {
    if let Some(b) = forced {
        return Ok(b.to_string());
    }
    CANDIDATE_BINARIES
        .iter()
        .find(|b| binary_exists(b))
        .map(|b| b.to_string())
        .context(
            "não encontrei `tofu` nem `terraform` no PATH. Instale um dos dois, \
             ou use --cli para apontar o binário certo, ou passe --plan com um JSON já exportado.",
        )
}

/// Roda `<bin> plan -out=<tmp>` seguido de `<bin> show -json <tmp>` dentro de
/// `dir`, devolvendo o JSON já pronto para o graph.rs consumir. É o caminho
/// "zero passos manuais": o próprio tfgraph gera o plan e converte pra JSON.
pub fn generate(dir: &Path, forced_bin: Option<&str>, extra_plan_args: &[String]) -> Result<Value> {
    let bin = resolve_binary(forced_bin)?;

    let tmp_plan = std::env::temp_dir().join(format!("tfgraph-plan-{}.bin", std::process::id()));

    eprintln!("$ {bin} plan -out={} (em {})", tmp_plan.display(), dir.display());
    let plan_status = Command::new(&bin)
        .arg("plan")
        .arg(format!("-out={}", tmp_plan.display()))
        .args(extra_plan_args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("falha ao executar `{bin} plan` em '{}'", dir.display()))?;

    if !plan_status.success() {
        let _ = std::fs::remove_file(&tmp_plan);
        bail!("`{bin} plan` terminou com erro (veja a saída acima)");
    }

    let value = show_json(&bin, &tmp_plan, Some(dir));
    let _ = std::fs::remove_file(&tmp_plan);
    value
}

fn show_json(bin: &str, plan_file: &Path, cwd: Option<&Path>) -> Result<Value> {
    let mut cmd = Command::new(bin);
    cmd.arg("show").arg("-json").arg(plan_file);
    // Crucial: `show -json` precisa rodar na mesma pasta onde `plan`/`init`
    // foram executados, senão não encontra os plugins de provider já
    // baixados em `.terraform/providers` e falha com "unavailable provider".
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let output = cmd
        .output()
        .with_context(|| format!("falha ao executar `{bin} show -json {}`", plan_file.display()))?;

    if !output.status.success() {
        bail!(
            "`{bin} show -json {}` falhou:\n{}",
            plan_file.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("saída de `{bin} show -json` não é um JSON válido"))
}

/// Carrega o plan a partir de um arquivo já existente.
///
/// Aceita dois formatos:
/// 1. Um JSON já exportado (`tofu show -json plan.tfplan > plan.json`)
/// 2. Um plan binário (`plan.tfplan` / `.out`) — nesse caso o tfgraph roda
///    `<bin> show -json <arquivo>` sozinho para obter o JSON.
pub fn load(path: &Path, forced_bin: Option<&str>) -> Result<Value> {
    let raw = std::fs::read(path)
        .with_context(|| format!("não consegui ler o arquivo '{}'", path.display()))?;

    // Tenta primeiro como JSON puro.
    if let Ok(value) = serde_json::from_slice::<Value>(&raw) {
        return Ok(value);
    }

    // Não é JSON: assume que é um plan binário e converte via CLI.
    // Roda `show` a partir da pasta onde o arquivo do plan está (heurística:
    // é normalmente a raiz do projeto onde `init`/`plan` foram rodados),
    // pra achar os plugins de provider corretos.
    let bin = resolve_binary(forced_bin)?;
    let cwd = path.parent().filter(|p| !p.as_os_str().is_empty());
    show_json(&bin, path, cwd)
}


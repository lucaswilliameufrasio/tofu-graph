use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Node {
    pub id: String,         // endereço completo (com índice), ex: aws_instance.web[0]
    pub config_key: String, // endereço sem índice, usado para casar dependências
    pub kind: String,       // ex: aws_instance
    pub name: String,       // ex: web
    pub module: String,     // ex: "" (root) ou "module.rede.module.subrede"
    pub action: String,     // create | update | delete | replace | no-op | read
    pub is_data: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Remove todos os índices `[...]` de um endereço, tanto de recursos
/// (`aws_instance.web[0]`) quanto de módulos (`module.x["a"].aws_instance.y`).
fn strip_indexes(addr: &str) -> String {
    let mut out = String::with_capacity(addr.len());
    let mut depth = 0usize;
    for c in addr.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn action_from_changes(actions: &[String]) -> String {
    let has = |a: &str| actions.iter().any(|x| x == a);
    if has("create") && has("delete") {
        "replace".to_string()
    } else if has("create") {
        "create".to_string()
    } else if has("delete") {
        "delete".to_string()
    } else if has("update") {
        "update".to_string()
    } else if has("read") {
        "read".to_string()
    } else {
        "no-op".to_string()
    }
}

/// Extrai a lista de nós a partir de `resource_changes` (plan) ou,
/// na falta dele, de `values.root_module` (state / plan sem changes).
fn collect_nodes(root: &Value) -> Vec<Node> {
    let mut nodes = Vec::new();

    if let Some(changes) = root.get("resource_changes").and_then(|v| v.as_array()) {
        for rc in changes {
            let addr = rc.get("address").and_then(|v| v.as_str()).unwrap_or("");
            if addr.is_empty() {
                continue;
            }
            let kind = rc
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = rc
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let module = rc
                .get("module_address")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_data = rc.get("mode").and_then(|v| v.as_str()) == Some("data");
            let actions: Vec<String> = rc
                .get("change")
                .and_then(|c| c.get("actions"))
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let action = action_from_changes(&actions);
            // "no-op" em resource_changes normalmente é recurso não afetado; ainda
            // assim mostramos, pois faz parte da infra provisionada.
            nodes.push(Node {
                id: addr.to_string(),
                config_key: strip_indexes(addr),
                kind,
                name,
                module,
                action,
                is_data,
            });
        }
    } else if let Some(root_module) = root
        .get("values")
        .and_then(|v| v.get("root_module"))
        .or_else(|| {
            root.get("planned_values")
                .and_then(|v| v.get("root_module"))
        })
    {
        collect_nodes_from_module(root_module, &mut nodes);
    }

    nodes
}

fn collect_nodes_from_module(module: &Value, out: &mut Vec<Node>) {
    if let Some(resources) = module.get("resources").and_then(|v| v.as_array()) {
        for r in resources {
            let addr = r.get("address").and_then(|v| v.as_str()).unwrap_or("");
            if addr.is_empty() {
                continue;
            }
            let kind = r
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = r
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_data = r.get("mode").and_then(|v| v.as_str()) == Some("data");
            out.push(Node {
                id: addr.to_string(),
                config_key: strip_indexes(addr),
                kind,
                name,
                module: String::new(),
                action: "no-op".to_string(),
                is_data,
            });
        }
    }
    if let Some(children) = module.get("child_modules").and_then(|v| v.as_array()) {
        for child in children {
            collect_nodes_from_module(child, out);
        }
    }
}

/// Dado um texto de referência do Terraform (ex: "aws_instance.web.id",
/// "data.aws_ami.ubuntu.id", "module.rede.aws_subnet.a"), devolve o
/// "endereço de recurso" correspondente (sem o atributo final), ou None
/// se a referência não aponta para um recurso (ex: var.*, local.*, each.*).
fn resource_ref_from_reference(reference: &str) -> Option<String> {
    let parts: Vec<&str> = reference.split('.').collect();
    if parts.is_empty() {
        return None;
    }
    match parts[0] {
        "var" | "local" | "each" | "count" | "path" | "terraform" | "self" => None,
        "data" => {
            if parts.len() >= 3 {
                Some(format!("data.{}.{}", parts[1], parts[2]))
            } else {
                None
            }
        }
        "module" => None, // saída de outro módulo: não é um recurso específico
        _ => {
            if parts.len() >= 2 {
                Some(format!("{}.{}", parts[0], parts[1]))
            } else {
                None
            }
        }
    }
}

/// Percorre recursivamente qualquer sub-árvore de `expressions`, coletando
/// todo array `"references"` encontrado (é assim que o Terraform expõe,
/// no JSON de configuração, quais atributos referenciam outros recursos).
fn collect_references(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(refs)) = map.get("references") {
                for r in refs {
                    if let Some(s) = r.as_str() {
                        out.push(s.to_string());
                    }
                }
            }
            for (k, v) in map {
                if k != "references" {
                    collect_references(v, out);
                }
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_references(v, out);
            }
        }
        _ => {}
    }
}

/// Percorre `configuration.root_module` (e módulos filhos) recursivamente
/// montando um mapa: endereço-de-config do recurso -> endereços-de-config
/// dos recursos dos quais ele depende (depends_on explícito + referências
/// implícitas encontradas nas expressions).
fn walk_config_module(module: &Value, prefix: &str, out: &mut HashMap<String, Vec<String>>) {
    if let Some(resources) = module.get("resources").and_then(|v| v.as_array()) {
        for r in resources {
            let local_addr = r.get("address").and_then(|v| v.as_str()).unwrap_or("");
            if local_addr.is_empty() {
                continue;
            }
            let full_addr = if prefix.is_empty() {
                local_addr.to_string()
            } else {
                format!("{}.{}", prefix, local_addr)
            };

            let mut raw_refs: Vec<String> = Vec::new();
            if let Some(dep) = r.get("depends_on").and_then(|v| v.as_array()) {
                for d in dep {
                    if let Some(s) = d.as_str() {
                        raw_refs.push(s.to_string());
                    }
                }
            }
            if let Some(expr) = r.get("expressions") {
                collect_references(expr, &mut raw_refs);
            }

            let mut targets: Vec<String> = raw_refs
                .iter()
                .filter_map(|r| resource_ref_from_reference(r))
                .map(|t| {
                    if prefix.is_empty() {
                        t
                    } else {
                        format!("{}.{}", prefix, t)
                    }
                })
                .collect();
            targets.sort();
            targets.dedup();

            out.entry(full_addr).or_default().extend(targets);
        }
    }

    if let Some(calls) = module.get("module_calls").and_then(|v| v.as_object()) {
        for (name, call) in calls {
            let new_prefix = if prefix.is_empty() {
                format!("module.{}", name)
            } else {
                format!("{}.module.{}", prefix, name)
            };
            if let Some(m) = call.get("module") {
                walk_config_module(m, &new_prefix, out);
            }
        }
    }
}

fn collect_dependencies(root: &Value) -> HashMap<String, Vec<String>> {
    let mut out = HashMap::new();
    if let Some(root_module) = root.get("configuration").and_then(|c| c.get("root_module")) {
        walk_config_module(root_module, "", &mut out);
    }
    out
}

pub fn build(root: &Value) -> Graph {
    let nodes = collect_nodes(root);
    let deps = collect_dependencies(root);

    // config_key -> lista de ids reais (pode haver mais de uma instância
    // por causa de count/for_each)
    let mut by_config: HashMap<String, Vec<String>> = HashMap::new();
    for n in &nodes {
        by_config
            .entry(n.config_key.clone())
            .or_default()
            .push(n.id.clone());
    }

    let mut seen_edges: HashSet<(String, String)> = HashSet::new();
    let mut edges = Vec::new();

    for n in &nodes {
        let Some(targets) = deps.get(&n.config_key) else {
            continue;
        };
        for target_key in targets {
            let Some(target_ids) = by_config.get(target_key) else {
                continue;
            };
            for target_id in target_ids {
                if target_id == &n.id {
                    continue;
                }
                let key = (n.id.clone(), target_id.clone());
                if seen_edges.insert(key.clone()) {
                    edges.push(Edge {
                        from: n.id.clone(),
                        to: target_id.clone(),
                    });
                }
            }
        }
    }

    Graph { nodes, edges }
}

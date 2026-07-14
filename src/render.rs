use crate::graph::Graph;
use serde_json::json;

/// Cor + rótulo de cada tipo de ação, espelhando os símbolos que o próprio
/// `terraform plan` usa no terminal (+ create, ~ update, - destroy, -/+ replace).
fn action_meta(action: &str) -> (&'static str, &'static str, &'static str) {
    // (cor, símbolo, rótulo em pt-br)
    match action {
        "create" => ("#3fb950", "+", "criar"),
        "update" => ("#d29922", "~", "atualizar"),
        "delete" => ("#f85149", "-", "destruir"),
        "replace" => ("#ff8c42", "±", "substituir"),
        "read" => ("#58a6ff", "?", "ler (data)"),
        _ => ("#6e7681", "=", "sem mudança"),
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn render(graph: &Graph, title: &str, plan_path: &str) -> String {
    let title = escape_html(title);
    let plan_path = escape_html(plan_path);
    let nodes_json: Vec<serde_json::Value> = graph
        .nodes
        .iter()
        .map(|n| {
            let (color, symbol, action_label) = action_meta(&n.action);
            let shape = if n.is_data { "hexagon" } else { "box" };
            let module_label = if n.module.is_empty() {
                "root".to_string()
            } else {
                n.module.clone()
            };
            json!({
                "id": n.id,
                "label": format!("{} {}\n{}", symbol, n.kind, n.name),
                "title": format!(
                    "{}\ntipo: {}\nnome: {}\nmódulo: {}\nação: {}",
                    n.id, n.kind, n.name, module_label, action_label
                ),
                "color": {
                    "background": if n.is_data { "#161b22" } else { color },
                    "border": color,
                    "highlight": { "background": color, "border": "#ffffff" }
                },
                "font": { "color": if n.is_data { color } else { "#0d1117" }, "face": "IBM Plex Mono, monospace", "multi": false, "size": 13 },
                "shape": shape,
                "margin": 10,
                "group": n.action,
            })
        })
        .collect();

    let edges_json: Vec<serde_json::Value> = graph
        .edges
        .iter()
        .map(|e| {
            json!({
                "from": e.from,
                "to": e.to,
                "arrows": "to",
                "color": { "color": "#30363d", "highlight": "#58a6ff", "hover": "#58a6ff" },
                "smooth": { "type": "cubicBezier", "roundness": 0.4 },
            })
        })
        .collect();

    let nodes_str = serde_json::to_string(&nodes_json).unwrap();
    let edges_str = serde_json::to_string(&edges_json).unwrap();

    let total_nodes = graph.nodes.len();
    let total_edges = graph.edges.len();

    format!(
        r##"<!doctype html>
<html lang="pt-br">
<head>
<meta charset="utf-8" />
<title>{title} — mapa de infraestrutura</title>
<meta name="viewport" content="width=device-width, initial-scale=1" />
<script src="https://unpkg.com/vis-network@9.1.9/standalone/umd/vis-network.min.js"></script>
<style>
  :root {{
    --bg: #0a0d12;
    --panel: #10151c;
    --line: #1c2530;
    --text: #c9d1d9;
    --muted: #6e7681;
    --accent: #58a6ff;
    --mono: 'IBM Plex Mono', 'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace;
    --sans: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  }}
  * {{ box-sizing: border-box; }}
  html, body {{
    margin: 0; padding: 0; height: 100%;
    background: var(--bg); color: var(--text);
    font-family: var(--sans);
  }}
  #topbar {{
    display: flex; align-items: baseline; gap: 16px;
    padding: 14px 20px; border-bottom: 1px solid var(--line);
    background: var(--panel);
  }}
  #topbar h1 {{
    font-family: var(--mono); font-size: 15px; font-weight: 600;
    margin: 0; color: #e6edf3; letter-spacing: 0.02em;
  }}
  #topbar .path {{
    font-family: var(--mono); font-size: 12px; color: var(--muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }}
  #topbar .stats {{
    margin-left: auto; font-family: var(--mono); font-size: 12px; color: var(--muted);
  }}
  #topbar .stats b {{ color: var(--text); }}
  #legend {{
    display: flex; gap: 18px; flex-wrap: wrap;
    padding: 10px 20px; border-bottom: 1px solid var(--line);
    background: #0d1117; font-family: var(--mono); font-size: 12px;
  }}
  .legend-item {{ display: flex; align-items: center; gap: 6px; color: var(--muted); }}
  .swatch {{
    width: 10px; height: 10px; border-radius: 2px; display: inline-block;
  }}
  #network {{
    width: 100%;
    height: calc(100vh - 96px);
    background:
      radial-gradient(circle at 1px 1px, #151b23 1px, transparent 1px) 0 0 / 22px 22px,
      var(--bg);
  }}
  #empty {{
    display: none; height: calc(100vh - 96px); align-items: center; justify-content: center;
    flex-direction: column; gap: 8px; font-family: var(--mono); color: var(--muted);
  }}
</style>
</head>
<body>
  <div id="topbar">
    <h1>▦ {title}</h1>
    <span class="path">{plan_path}</span>
    <span class="stats"><b>{total_nodes}</b> recursos · <b>{total_edges}</b> dependências</span>
  </div>
  <div id="legend">
    <span class="legend-item"><span class="swatch" style="background:#3fb950"></span>+ criar</span>
    <span class="legend-item"><span class="swatch" style="background:#d29922"></span>~ atualizar</span>
    <span class="legend-item"><span class="swatch" style="background:#f85149"></span>- destruir</span>
    <span class="legend-item"><span class="swatch" style="background:#ff8c42"></span>± substituir</span>
    <span class="legend-item"><span class="swatch" style="background:#58a6ff"></span>? data source</span>
    <span class="legend-item"><span class="swatch" style="background:#6e7681"></span>= sem mudança</span>
    <span class="legend-item" style="margin-left:auto">arraste os nós · role para zoom · clique para focar dependências</span>
  </div>
  <div id="network"></div>
  <div id="empty">
    <div style="font-size:28px">∅</div>
    <div>nenhum recurso encontrado neste plan</div>
  </div>

<script>
  const rawNodes = {nodes_str};
  const rawEdges = {edges_str};

  if (rawNodes.length === 0) {{
    document.getElementById('network').style.display = 'none';
    document.getElementById('empty').style.display = 'flex';
  }} else {{
    const nodes = new vis.DataSet(rawNodes);
    const edges = new vis.DataSet(rawEdges);
    const container = document.getElementById('network');

    const options = {{
      layout: {{ improvedLayout: true }},
      physics: {{
        solver: 'forceAtlas2Based',
        forceAtlas2Based: {{ gravitationalConstant: -60, springLength: 140, springConstant: 0.06, avoidOverlap: 0.6 }},
        stabilization: {{ iterations: 200 }},
      }},
      interaction: {{ hover: true, tooltipDelay: 120, navigationButtons: true, keyboard: true }},
      nodes: {{ borderWidth: 2, shadow: false }},
      edges: {{ width: 1.4, selectionWidth: 2 }},
    }};

    const network = new vis.Network(container, {{ nodes, edges }}, options);

    network.on('selectNode', function (params) {{
      const id = params.nodes[0];
      const connected = new Set([id, ...network.getConnectedNodes(id)]);
      nodes.update(rawNodes.map(n => ({{
        id: n.id,
        opacity: connected.has(n.id) ? 1 : 0.15,
      }})));
      edges.update(rawEdges.map((e, i) => ({{
        id: edges.getIds()[i],
        color: {{ color: (e.from === id || e.to === id) ? '#58a6ff' : '#1c2530' }},
      }})));
    }});
    network.on('deselectNode', function () {{
      nodes.update(rawNodes.map(n => ({{ id: n.id, opacity: 1 }})));
      edges.update(rawEdges);
    }});
  }}
</script>
</body>
</html>
"##,
        title = title,
        plan_path = plan_path,
        total_nodes = total_nodes,
        total_edges = total_edges,
        nodes_str = nodes_str,
        edges_str = edges_str,
    )
}

#[cfg(test)]
mod tests {
    use super::escape_html;

    #[test]
    fn escapes_html_special_characters() {
        assert_eq!(
            escape_html("<title> & \"quoted\" 'value'"),
            "&lt;title&gt; &amp; &quot;quoted&quot; &#39;value&#39;"
        );
    }
}

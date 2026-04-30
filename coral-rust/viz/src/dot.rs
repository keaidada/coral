// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

use crate::walker::{Node, NodeKind};

pub fn render(roots: &[Node]) -> String {
    let mut out = String::new();
    out.push_str("digraph coral_ast {\n");
    out.push_str("  rankdir=TB;\n");
    out.push_str("  node [fontname=\"Helvetica\", shape=box, style=\"rounded,filled\"];\n");
    out.push_str("  edge [fontname=\"Helvetica\"];\n\n");

    let mut counter: u32 = 0;
    for (i, root) in roots.iter().enumerate() {
        out.push_str(&format!("  // statement {i}\n"));
        let _ = write_node(root, &mut counter, None, &mut out);
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

fn write_node(node: &Node, counter: &mut u32, parent: Option<u32>, out: &mut String) -> u32 {
    let id = *counter;
    *counter += 1;
    let color = kind_color(node.kind);
    let shape = kind_shape(node.kind);
    let label = escape(&node.label);
    out.push_str(&format!(
        "  n{id} [label=\"{label}\", fillcolor=\"{color}\", shape={shape}];\n"
    ));
    if let Some(p) = parent {
        out.push_str(&format!("  n{p} -> n{id};\n"));
    }
    for child in &node.children {
        write_node(child, counter, Some(id), out);
    }
    id
}

fn kind_color(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Statement | NodeKind::Query | NodeKind::Subquery | NodeKind::Cte => "#E8EAF6",
        NodeKind::Select | NodeKind::Projection => "#E3F2FD",
        NodeKind::From | NodeKind::Table | NodeKind::Join => "#FFF3E0",
        NodeKind::Filter | NodeKind::Having => "#FCE4EC",
        NodeKind::GroupBy | NodeKind::OrderBy | NodeKind::Limit => "#F1F8E9",
        NodeKind::SetOp => "#EDE7F6",
        NodeKind::Expression => "#FFFFFF",
        NodeKind::Values | NodeKind::Misc => "#EEEEEE",
    }
}

fn kind_shape(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Table => "cylinder",
        NodeKind::Expression => "ellipse",
        _ => "box",
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

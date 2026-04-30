// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

use crate::walker::{Node, NodeKind};

pub fn render(roots: &[Node]) -> String {
    let mut out = String::from("@startuml\n");
    out.push_str("skinparam rectangleBackgroundColor #FAFAFA\n");
    out.push_str("skinparam rectangleBorderColor #BDBDBD\n");
    out.push_str("skinparam roundCorner 10\n\n");

    let mut counter: u32 = 0;
    for (i, root) in roots.iter().enumerate() {
        out.push_str(&format!("' statement {i}\n"));
        write_node(root, &mut counter, None, &mut out);
        out.push('\n');
    }
    out.push_str("@enduml\n");
    out
}

fn write_node(node: &Node, counter: &mut u32, parent: Option<u32>, out: &mut String) {
    let id = *counter;
    *counter += 1;
    let color = kind_color(node.kind);
    let label = escape(&node.label);
    out.push_str(&format!("rectangle \"{label}\" as n{id} {color}\n"));
    if let Some(p) = parent {
        out.push_str(&format!("n{p} --> n{id}\n"));
    }
    for child in &node.children {
        write_node(child, counter, Some(id), out);
    }
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

fn escape(s: &str) -> String {
    // PlantUML's rectangle label accepts quoted strings; escape embedded
    // quotes by doubling them (PlantUML convention) and normalize
    // newlines.
    s.replace('"', "''").replace('\n', " ")
}

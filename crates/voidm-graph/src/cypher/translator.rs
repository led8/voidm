use super::ast::*;
use anyhow::{bail, Result};
use std::collections::HashMap;

/// Translate a CypherAst into a SQL query string and bind parameters.
pub fn translate(ast: &CypherAst) -> Result<(String, Vec<serde_json::Value>)> {
    let mut params: Vec<serde_json::Value> = Vec::new();
    let mut ctx = TranslateCtx::new();

    if ast.match_clauses.is_empty() {
        bail!("Query must have at least one MATCH clause");
    }
    if ast.return_exprs.is_empty() {
        bail!("Query must have a RETURN clause");
    }

    // Build FROM + JOINs for each match clause
    let mut from_parts = Vec::new();
    let mut join_parts = Vec::new();

    for (ci, mc) in ast.match_clauses.iter().enumerate() {
        translate_pattern(
            &mc.pattern,
            ci,
            &mut ctx,
            &mut from_parts,
            &mut join_parts,
            &mut params,
        )?;
    }

    // SELECT
    let select_cols = translate_return(&ast.return_exprs, &ctx)?;

    // WHERE
    let mut where_parts = Vec::new();
    // Constraints collected during pattern translation
    for c in &ctx.constraints {
        where_parts.push(c.clone());
    }
    // Explicit WHERE clause
    if let Some(ref w) = ast.where_clause {
        let (w_sql, w_params) = translate_where(w, &ctx)?;
        where_parts.push(w_sql);
        params.extend(w_params);
    }

    // ORDER BY
    let order_sql = if !ast.order_by.is_empty() {
        let parts: Vec<String> = ast
            .order_by
            .iter()
            .map(|o| {
                let col = return_item_to_col(&o.expr, &ctx).unwrap_or_else(|_| "1".into());
                if o.desc {
                    format!("{} DESC", col)
                } else {
                    col
                }
            })
            .collect();
        format!(" ORDER BY {}", parts.join(", "))
    } else {
        String::new()
    };

    // LIMIT
    let limit_sql = if let Some(n) = ast.limit {
        format!(" LIMIT {}", n)
    } else {
        " LIMIT 100".into() // default safety limit
    };

    // Assemble
    let from_sql = from_parts.join(", ");
    let join_sql = join_parts.join(" ");
    let where_sql = if where_parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_parts.join(" AND "))
    };

    let sql = format!(
        "SELECT {} FROM {} {} {}{}{}",
        select_cols, from_sql, join_sql, where_sql, order_sql, limit_sql
    );

    Ok((sql, params))
}

struct TranslateCtx {
    /// Map from Cypher variable name → SQL table alias
    node_aliases: HashMap<String, String>,
    /// Map from Cypher variable name → node kind ("Memory" | "Concept")
    node_kinds: HashMap<String, String>,
    /// Map from Cypher edge variable name → SQL table alias for graph_edges / ontology_edges
    edge_aliases: HashMap<String, String>,
    /// Map from Cypher edge variable name → edge table kind ("graph" | "ontology") — reserved for future cross-graph edge routing
    #[allow(dead_code)]
    edge_table_kinds: HashMap<String, String>,
    /// Constraints to add to WHERE
    constraints: Vec<String>,
    counter: usize,
}

impl TranslateCtx {
    fn new() -> Self {
        Self {
            node_aliases: HashMap::new(),
            node_kinds: HashMap::new(),
            edge_aliases: HashMap::new(),
            edge_table_kinds: HashMap::new(),
            constraints: Vec::new(),
            counter: 0,
        }
    }

    fn fresh(&mut self, prefix: &str) -> String {
        let n = self.counter;
        self.counter += 1;
        format!("{}_{}", prefix, n)
    }

    fn is_concept(&self, var: &str) -> bool {
        self.node_kinds
            .get(var)
            .map(|k| k == "Concept")
            .unwrap_or(false)
    }
}

fn translate_pattern(
    pattern: &Pattern,
    _clause_idx: usize,
    ctx: &mut TranslateCtx,
    from_parts: &mut Vec<String>,
    join_parts: &mut Vec<String>,
    params: &mut Vec<serde_json::Value>,
) -> Result<()> {
    if pattern.nodes.is_empty() {
        return Ok(());
    }

    // First node
    let n0_alias = register_node(&pattern.nodes[0], ctx, from_parts, join_parts, params)?;

    for (i, edge) in pattern.edges.iter().enumerate() {
        let n1 = &pattern.nodes[i + 1];

        let e_alias = ctx.fresh("e");
        let n1_alias = ctx.fresh("n");

        // Register edge variable so r.rel_type etc. resolve correctly
        if let Some(ref var) = edge.var {
            ctx.edge_aliases.insert(var.clone(), e_alias.clone());
        }

        // Edge table
        match edge.direction {
            EdgeDirection::Outgoing => {
                join_parts.push(format!(
                    "JOIN graph_edges {} ON {}.source_id = {}.id",
                    e_alias, e_alias, n0_alias
                ));
            }
            EdgeDirection::Incoming => {
                join_parts.push(format!(
                    "JOIN graph_edges {} ON {}.target_id = {}.id",
                    e_alias, e_alias, n0_alias
                ));
            }
            EdgeDirection::Undirected => {
                // RELATES_TO or any undirected — use a subquery UNION
                // For simplicity: join both directions
                let sub_alias = ctx.fresh("sub");
                join_parts.push(format!(
                    "JOIN (SELECT source_id AS a_id, target_id AS b_id, rel_type, id FROM graph_edges \
                     UNION SELECT target_id, source_id, rel_type, id FROM graph_edges) {sub} ON {sub}.a_id = {n0}.id",
                    sub = sub_alias, n0 = n0_alias
                ));
                // n1 node joins on sub.b_id
                join_parts.push(format!(
                    "JOIN graph_nodes {} ON {}.id = {}.b_id",
                    n1_alias, n1_alias, sub_alias
                ));

                // Edge type constraint
                if let Some(ref rt) = edge.rel_type {
                    ctx.constraints
                        .push(format!("{}.rel_type = '{}'", sub_alias, rt));
                }

                // Register n1 alias
                if let Some(ref var) = n1.var {
                    ctx.node_aliases.insert(var.clone(), n1_alias.clone());
                }
                // Label constraint for n1 — skip :Memory (cosmetic)
                if let Some(ref label) = n1.label {
                    if label != "Memory" {
                        let lbl_alias = ctx.fresh("lbl");
                        join_parts.push(format!(
                            "JOIN graph_node_labels {} ON {}.node_id = {}.id AND {}.label = '{}'",
                            lbl_alias, lbl_alias, n1_alias, lbl_alias, label
                        ));
                    }
                }
                continue;
            }
        }

        // Target node (for directed)
        let target_col = match edge.direction {
            EdgeDirection::Outgoing => "target_id",
            EdgeDirection::Incoming => "source_id",
            EdgeDirection::Undirected => unreachable!(),
        };
        join_parts.push(format!(
            "JOIN graph_nodes {} ON {}.id = {}.{}",
            n1_alias, n1_alias, e_alias, target_col
        ));

        // Edge type constraint
        if let Some(ref rt) = edge.rel_type {
            ctx.constraints
                .push(format!("{}.rel_type = '{}'", e_alias, rt));
        }

        // n1 label constraint — skip :Memory (cosmetic)
        if let Some(ref label) = n1.label {
            if label != "Memory" {
                let lbl_alias = ctx.fresh("lbl");
                join_parts.push(format!(
                    "JOIN graph_node_labels {} ON {}.node_id = {}.id AND {}.label = '{}'",
                    lbl_alias, lbl_alias, n1_alias, lbl_alias, label
                ));
            }
        }

        // Register n1 variable
        if let Some(ref var) = n1.var {
            ctx.node_aliases.insert(var.clone(), n1_alias.clone());
        }

        // n1 property constraints
        for (key, val) in &n1.props {
            let pk_alias = ctx.fresh("pk");
            let pv_alias = ctx.fresh("pv");
            join_parts.push(format!(
                "JOIN graph_property_keys {} ON {}.key = '{}'",
                pk_alias, pk_alias, key
            ));
            join_parts.push(format!(
                "JOIN graph_node_props_text {} ON {}.node_id = {}.id AND {}.key_id = {}.id",
                pv_alias, pv_alias, n1_alias, pv_alias, pk_alias
            ));
            match val {
                PropValue::String(s) => {
                    ctx.constraints.push(format!(
                        "{}.value = '{}'",
                        pv_alias,
                        s.replace('\'', "''")
                    ));
                }
                PropValue::Integer(n) => {
                    ctx.constraints.push(format!("{}.value = {}", pv_alias, n));
                }
                PropValue::Bool(b) => {
                    ctx.constraints.push(format!(
                        "{}.value = {}",
                        pv_alias,
                        if *b { 1 } else { 0 }
                    ));
                }
                PropValue::Null => {}
            }
        }
    }

    Ok(())
}

fn register_node(
    node: &NodeSpec,
    ctx: &mut TranslateCtx,
    from_parts: &mut Vec<String>,
    join_parts: &mut Vec<String>,
    _params: &mut Vec<serde_json::Value>,
) -> Result<String> {
    let alias = ctx.fresh("n");

    let label = node.label.as_deref().unwrap_or("Memory");
    let is_concept = label == "Concept";

    if let Some(ref var) = node.var {
        ctx.node_aliases.insert(var.clone(), alias.clone());
        ctx.node_kinds.insert(var.clone(), label.to_string());
    }

    if is_concept {
        // Route to ontology_concepts table
        from_parts.push(format!("ontology_concepts {}", alias));

        // Property constraints on concept columns
        for (key, val) in &node.props {
            let col = match key.as_str() {
                "id" | "concept_id" => format!("{}.id", alias),
                "name" => format!("{}.name", alias),
                "description" => format!("{}.description", alias),
                "scope" => format!("{}.scope", alias),
                _ => continue,
            };
            match val {
                PropValue::String(s) => {
                    ctx.constraints
                        .push(format!("{} = '{}'", col, s.replace('\'', "''")));
                }
                _ => {}
            }
        }
    } else {
        // Memory node → graph_nodes table
        from_parts.push(format!("graph_nodes {}", alias));

        if let Some(ref label_str) = node.label {
            if label_str != "Memory" {
                let lbl_alias = ctx.fresh("lbl");
                join_parts.push(format!(
                    "JOIN graph_node_labels {} ON {}.node_id = {}.id AND {}.label = '{}'",
                    lbl_alias, lbl_alias, alias, lbl_alias, label_str
                ));
            }
        }

        // Property constraints on memory node
        for (key, val) in &node.props {
            let pk_alias = ctx.fresh("pk");
            let pv_alias = ctx.fresh("pv");
            join_parts.push(format!(
                "JOIN graph_property_keys {} ON {}.key = '{}'",
                pk_alias, pk_alias, key
            ));
            join_parts.push(format!(
                "JOIN graph_node_props_text {} ON {}.node_id = {}.id AND {}.key_id = {}.id",
                pv_alias, pv_alias, alias, pv_alias, pk_alias
            ));
            match val {
                PropValue::String(s) => {
                    ctx.constraints.push(format!(
                        "{}.value = '{}'",
                        pv_alias,
                        s.replace('\'', "''")
                    ));
                }
                PropValue::Integer(n) => {
                    ctx.constraints.push(format!("{}.value = {}", pv_alias, n));
                }
                _ => {}
            }
        }
    }

    Ok(alias)
}

fn translate_return(exprs: &[ReturnExpr], ctx: &TranslateCtx) -> Result<String> {
    let parts: Vec<String> = exprs
        .iter()
        .map(|re| {
            let col = return_item_to_col_ctx(&re.expr, ctx);
            if let Some(ref alias) = re.alias {
                format!("{} AS \"{}\"", col, alias)
            } else {
                col
            }
        })
        .collect();
    Ok(parts.join(", "))
}

fn return_item_to_col_ctx(item: &ReturnItem, ctx: &TranslateCtx) -> String {
    match item {
        ReturnItem::Property(var, prop) => {
            if let Some(node_alias) = ctx.node_aliases.get(var.as_str()) {
                if ctx.is_concept(var) {
                    // Concept node — direct column access
                    return match prop.as_str() {
                        "id" | "concept_id" => format!("{}.id", node_alias),
                        "name" => format!("{}.name", node_alias),
                        "description" => format!("{}.description", node_alias),
                        "scope" => format!("{}.scope", node_alias),
                        "created_at" => format!("{}.created_at", node_alias),
                        _ => format!("NULL /* unknown concept prop {} */", prop),
                    };
                } else {
                    // Memory node — id maps to memory_id, others via props tables
                    if prop == "id" || prop == "memory_id" {
                        return format!("{}.memory_id", node_alias);
                    }
                    return format!(
                        "(SELECT value FROM graph_node_props_text pt \
                          JOIN graph_property_keys pk ON pk.id = pt.key_id \
                          WHERE pt.node_id = {n}.id AND pk.key = '{p}')",
                        n = node_alias,
                        p = prop
                    );
                }
            }
            // Edge alias
            if let Some(edge_alias) = ctx.edge_aliases.get(var.as_str()) {
                return match prop.as_str() {
                    "rel_type" | "type" => format!("{}.rel_type", edge_alias),
                    "note" => format!("{}.note", edge_alias),
                    "id" => format!("{}.id", edge_alias),
                    _ => format!("NULL /* unknown edge prop {} */", prop),
                };
            }
            format!("NULL /* unknown var {} */", var)
        }
        ReturnItem::Variable(var) => {
            if let Some(node_alias) = ctx.node_aliases.get(var.as_str()) {
                if ctx.is_concept(var) {
                    format!("{}.id", node_alias)
                } else {
                    format!("{}.memory_id", node_alias)
                }
            } else if let Some(edge_alias) = ctx.edge_aliases.get(var.as_str()) {
                format!("{}.rel_type", edge_alias)
            } else {
                format!("NULL /* unknown var {} */", var)
            }
        }
        ReturnItem::Count(inner) => match inner {
            Some(v) => format!("COUNT({})", v),
            None => "COUNT(*)".into(),
        },
    }
}

fn return_item_to_col(item: &ReturnItem, ctx: &TranslateCtx) -> Result<String> {
    Ok(return_item_to_col_ctx(item, ctx))
}

fn translate_where(
    expr: &WhereExpr,
    ctx: &TranslateCtx,
) -> Result<(String, Vec<serde_json::Value>)> {
    match expr {
        WhereExpr::Comparison(cmp) => {
            let col = if let Some(node_alias) = ctx.node_aliases.get(&cmp.var) {
                if ctx.is_concept(&cmp.var) {
                    match cmp.prop.as_str() {
                        "id" | "concept_id" => format!("{}.id", node_alias),
                        "name" => format!("{}.name", node_alias),
                        "description" => format!("{}.description", node_alias),
                        "scope" => format!("{}.scope", node_alias),
                        _ => bail!("Unknown concept property '{}' in WHERE", cmp.prop),
                    }
                } else if cmp.prop == "id" || cmp.prop == "memory_id" {
                    format!("{}.memory_id", node_alias)
                } else {
                    format!(
                        "(SELECT value FROM graph_node_props_text pt \
                          JOIN graph_property_keys pk ON pk.id = pt.key_id \
                          WHERE pt.node_id = {n}.id AND pk.key = '{p}')",
                        n = node_alias,
                        p = cmp.prop
                    )
                }
            } else {
                bail!("Unknown variable '{}' in WHERE clause", cmp.var);
            };

            let (op_sql, val_params) = match &cmp.op {
                CompOp::Eq => {
                    let v = prop_to_json(&cmp.value);
                    (format!("{} = ?", col), vec![v])
                }
                CompOp::Contains => {
                    if let PropValue::String(s) = &cmp.value {
                        (
                            format!("{} LIKE ?", col),
                            vec![serde_json::json!(format!("%{}%", s))],
                        )
                    } else {
                        bail!("CONTAINS requires a string value");
                    }
                }
                CompOp::StartsWith => {
                    if let PropValue::String(s) = &cmp.value {
                        (
                            format!("{} LIKE ?", col),
                            vec![serde_json::json!(format!("{}%", s))],
                        )
                    } else {
                        bail!("STARTS WITH requires a string value");
                    }
                }
                CompOp::EndsWith => {
                    if let PropValue::String(s) = &cmp.value {
                        (
                            format!("{} LIKE ?", col),
                            vec![serde_json::json!(format!("%{}", s))],
                        )
                    } else {
                        bail!("ENDS WITH requires a string value");
                    }
                }
                _ => {
                    let v = prop_to_json(&cmp.value);
                    (format!("{} = ?", col), vec![v])
                }
            };

            Ok((op_sql, val_params))
        }
        WhereExpr::And(l, r) => {
            let (ls, lp) = translate_where(l, ctx)?;
            let (rs, rp) = translate_where(r, ctx)?;
            let mut p = lp;
            p.extend(rp);
            Ok((format!("({} AND {})", ls, rs), p))
        }
        WhereExpr::Or(l, r) => {
            let (ls, lp) = translate_where(l, ctx)?;
            let (rs, rp) = translate_where(r, ctx)?;
            let mut p = lp;
            p.extend(rp);
            Ok((format!("({} OR {})", ls, rs), p))
        }
        WhereExpr::Not(inner) => {
            let (s, p) = translate_where(inner, ctx)?;
            Ok((format!("NOT ({})", s), p))
        }
    }
}

fn prop_to_json(v: &PropValue) -> serde_json::Value {
    match v {
        PropValue::String(s) => serde_json::Value::String(s.clone()),
        PropValue::Integer(n) => serde_json::json!(n),
        PropValue::Bool(b) => serde_json::json!(b),
        PropValue::Null => serde_json::Value::Null,
    }
}

/// Full AST for a read-only Cypher query.
#[derive(Debug, Clone)]
pub struct CypherAst {
    pub match_clauses: Vec<MatchClause>,
    pub where_clause: Option<WhereExpr>,
    pub return_exprs: Vec<ReturnExpr>,
    pub order_by: Vec<OrderBy>,
    pub limit: Option<i64>,
}

/// A MATCH clause: one pattern.
#[derive(Debug, Clone)]
pub struct MatchClause {
    pub pattern: Pattern,
    pub optional: bool,
}

/// A path pattern like (a:Memory)-[:SUPPORTS]->(b:Memory)
#[derive(Debug, Clone)]
pub struct Pattern {
    pub nodes: Vec<NodeSpec>,
    pub edges: Vec<EdgeSpec>,
}

#[derive(Debug, Clone)]
pub struct NodeSpec {
    pub var: Option<String>,
    pub label: Option<String>,
    pub props: Vec<(String, PropValue)>,
}

#[derive(Debug, Clone)]
pub struct EdgeSpec {
    pub var: Option<String>,
    pub rel_type: Option<String>,
    pub direction: EdgeDirection,
    pub min_hops: Option<u32>,
    pub max_hops: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EdgeDirection {
    Outgoing,   // -[]->(
    Incoming,   // <-[]-
    Undirected, // -[]-
}

#[derive(Debug, Clone)]
pub enum PropValue {
    String(String),
    Integer(i64),
    Bool(bool),
    Null,
}

/// WHERE expression (simplified: AND/OR of comparisons)
#[derive(Debug, Clone)]
pub enum WhereExpr {
    Comparison(Comparison),
    And(Box<WhereExpr>, Box<WhereExpr>),
    Or(Box<WhereExpr>, Box<WhereExpr>),
    Not(Box<WhereExpr>),
}

#[derive(Debug, Clone)]
pub struct Comparison {
    pub var: String,
    pub prop: String,
    pub op: CompOp,
    pub value: PropValue,
}

#[derive(Debug, Clone)]
pub enum CompOp {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    Contains,
    StartsWith,
    EndsWith,
    IsNull,
    IsNotNull,
}

/// RETURN expression
#[derive(Debug, Clone)]
pub struct ReturnExpr {
    pub expr: ReturnItem,
    pub alias: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ReturnItem {
    Property(String, String), // var.prop
    Variable(String),         // whole node/var
    Count(Option<String>),    // COUNT(*) or COUNT(var)
}

#[derive(Debug, Clone)]
pub struct OrderBy {
    pub expr: ReturnItem,
    pub desc: bool,
}

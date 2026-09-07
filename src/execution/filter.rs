//! # Filter
//!
//! Execution tree node for `where` clause filtering.
//! I wanna call it restrict, but it's so hard to type.
//!

use crate::{
    execution::{
        ExecErr,
        query::{Operator, Row, Schema, literal_to_col_data},
    },
    interface::parser::{ExprNode, OpExpr, WhereNode},
    storage::{ColData, Type},
};

pub struct Filter<'a> {
    source: Box<dyn Operator + 'a>,
    preds: Vec<Predicate>,
}

// Should interpret as: `<col> <op> <other>`
// e.g. col = c1, op = GT, other = 2 => "c1 > 2"
// requires: col and other should already have same type
struct Predicate {
    col: String,    // One of them must be column, otherwise, what's the point?
    op: PredOp,     // We have our own pred enum
    other: ColData, // Literal for now

    // idx we'll use to get the col data out of row
    col_idx: usize,
}

/// Predicate's enum of operator.
enum PredOp {
    Eq,
    Neq,
    GT,
    GTE,
    LT,
    LTE,
}

impl<'a> Operator for Filter<'a> {
    fn next(&mut self) -> Result<Option<super::query::Row>, ExecErr> {
        loop {
            match self.source.next() {
                Ok(Some(row)) => {
                    if self.satisfy_all(&row) {
                        return Ok(Some(row));
                    }
                    continue;
                }
                otherwise => {
                    return otherwise;
                }
            }
        }
    }

    fn schema(&self) -> &Schema {
        self.source.schema()
    }
}

impl<'a> Filter<'a> {
    /// Create filter operator from source + parse the parse tree's conds node
    pub fn new(source: Box<dyn Operator + 'a>, conds: WhereNode) -> Result<Self, ExecErr> {
        let mut preds: Vec<Predicate> = Vec::with_capacity(conds.preds.len());
        let schema = source.schema();
        for expr in conds.preds {
            preds.push(expr_to_pred(expr, schema)?);
        }
        Ok(Filter { source, preds })
    }

    /// Check if row satisfies all predicates registered in Filter
    fn satisfy_all(&self, row: &Row) -> bool {
        for (i, _) in self.preds.iter().enumerate() {
            if !self.satisfy(row, i) {
                return false;
            }
        }
        return true;
    }

    /// Check if row satisfies pred at idx `i` in Filter
    fn satisfy(&self, row: &Row, pred_idx: usize) -> bool {
        let pred = &self.preds[pred_idx];
        let col_data = &row.data[pred.col_idx];
        let other = &pred.other;
        use PredOp::*;
        match pred.op {
            Eq => col_data == other,
            Neq => col_data != other,
            GT => col_data > other,
            GTE => col_data >= other,
            LT => col_data < other,
            LTE => col_data <= other,
        }
    }
}

/// Helper that map ExprNode to Predicate
// requires: expr must be op that is .is_pred()
fn expr_to_pred(expr: ExprNode, schema: &Schema) -> Result<Predicate, ExecErr> {
    debug_assert!(matches!(expr, ExprNode::Op(_)));

    let op_expr = match expr {
        ExprNode::Op(op_expr) => op_expr,
        _ => unreachable!("Must be operator expression."),
    };
    debug_assert!(op_expr.is_pred());

    let op: PredOp = (&op_expr).into();
    match op_expr {
        OpExpr::Eq(lhs, rhs)
        | OpExpr::Neq(lhs, rhs)
        | OpExpr::GT(lhs, rhs)
        | OpExpr::GTE(lhs, rhs)
        | OpExpr::LT(lhs, rhs)
        | OpExpr::LTE(lhs, rhs) => match (*lhs, *rhs) {
            // LHS column, RHS literal expression
            (ExprNode::Column(col_node), ExprNode::Literal(lit_expr)) => {
                let col = col_node.name;
                let (col_idx, col_type) = schema
                    .find_column(col.as_str())
                    .ok_or_else(|| ExecErr::InvalidColName(col.clone()))?;
                let other = literal_to_col_data(lit_expr, col_type)?;
                Ok(Predicate {
                    col,
                    op,
                    other,
                    col_idx,
                })
            }
            // LHS literal expression, RHS column
            (ExprNode::Literal(lit_expr), ExprNode::Column(col_node)) => {
                let col = col_node.name;
                let (col_idx, col_type) = schema
                    .find_column(col.as_str())
                    .ok_or_else(|| ExecErr::InvalidColName(col.clone()))?;
                let other = literal_to_col_data(lit_expr, col_type)?;
                // Since column is on the right side, we have to reverse the operator
                let op = match op {
                    PredOp::Eq => PredOp::Eq,
                    PredOp::Neq => PredOp::Neq,
                    PredOp::GT => PredOp::LT,
                    PredOp::GTE => PredOp::LTE,
                    PredOp::LT => PredOp::GT,
                    PredOp::LTE => PredOp::GTE,
                };
                Ok(Predicate {
                    col,
                    op,
                    other,
                    col_idx,
                })
            }
            _ => {
                return Err(ExecErr::InvalidPredicate);
            }
        },
        _ => unreachable!("Must be predable operator."),
    }
}

impl From<&OpExpr> for PredOp {
    // Requires: op should be .is_pred()
    fn from(op: &OpExpr) -> Self {
        match op {
            OpExpr::Eq(_, _) => PredOp::Eq,
            OpExpr::Neq(_, _) => PredOp::Neq,
            OpExpr::GT(_, _) => PredOp::GT,
            OpExpr::GTE(_, _) => PredOp::GTE,
            OpExpr::LT(_, _) => PredOp::LT,
            OpExpr::LTE(_, _) => PredOp::LTE,
            _ => unreachable!("Shouldn't be non predable because is_pred()"),
        }
    }
}

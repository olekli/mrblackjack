// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use crate::error::AssertDiagnostic;
use crate::match_object;
use crate::test_spec::Expr;
use serde_json;

pub fn assert_expr(input: &Vec<&serde_json::Value>, expr: &Expr) -> Result<(), AssertDiagnostic> {
    log::trace!("checking for condition {expr:?}");
    match expr {
        Expr::OneExpr { one } => input
            .iter()
            .any(|item| match_object::contains(item, one))
            .then_some(())
            .ok_or_else(|| AssertDiagnostic {
                input: input.iter().cloned().cloned().collect(),
                expr: expr.clone(),
            }),
        Expr::AllExpr { all } => input
            .iter()
            .all(|item| match_object::contains(item, all))
            .then_some(())
            .ok_or_else(|| AssertDiagnostic {
                input: input.iter().cloned().cloned().collect(),
                expr: expr.clone(),
            }),
        Expr::SizeExpr { size } => {
            (input.len() == *size)
                .then_some(())
                .ok_or_else(|| AssertDiagnostic {
                    input: vec![serde_json::json!(input.len())],
                    expr: expr.clone(),
                })
        }
        Expr::AndExpr { and } => and
            .iter()
            .map(|e| assert_expr(input, e))
            .collect::<Result<Vec<()>, AssertDiagnostic>>()
            .map(|_| ()),
        Expr::OrExpr { or } => or
            .iter()
            .any(|e| assert_expr(input, e).is_ok())
            .then_some(())
            .ok_or_else(|| AssertDiagnostic {
                input: input.iter().cloned().cloned().collect(),
                expr: expr.clone(),
            }),
        Expr::NotExpr { not } => assert_expr(input, not)
            .is_err()
            .then_some(())
            .ok_or_else(|| AssertDiagnostic {
                input: input.iter().cloned().cloned().collect(),
                expr: expr.clone(),
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::json;

    #[rstest]
    #[case(
        vec![json!({"status": "Ready"}), json!({"status": "NotReady"})],
        Expr::OneExpr { one: json!({"status": "Ready"}) },
        true
    )]
    #[case(
        vec![json!({"status": "Ready"}), json!({"status": "Ready"})],
        Expr::AllExpr { all: json!({"status": "Ready"}) },
        true
    )]
    #[case(
        vec![json!({"status": "Ready"}), json!({"status": "NotReady"})],
        Expr::AllExpr { all: json!({"status": "Ready"}) },
        false
    )]
    #[case(
        vec![json!({"status": "Ready"}), json!({"status": "Ready"})],
        Expr::SizeExpr { size: 2 },
        true
    )]
    #[case(
        vec![json!({"status": "Ready"}), json!({"status": "Ready"})],
        Expr::SizeExpr { size: 1 },
        false
    )]
    #[case(
        vec![json!({"status": "Ready"})],
        Expr::NotExpr { not: Box::new(Expr::SizeExpr { size: 0 }) },
        true
    )]
    #[case(
        vec![json!({"status": "Ready"}), json!({"status": "NotReady"})],
        Expr::AndExpr { and: vec![
            Expr::SizeExpr { size: 2 },
            Expr::OneExpr { one: json!({"status": "Ready"}) },
            Expr::OneExpr { one: json!({"status": "NotReady"}) },
        ] },
        true
    )]
    #[case(
        vec![json!({"status": "Ready"})],
        Expr::OrExpr { or: vec![
            Expr::SizeExpr { size: 0 },
            Expr::SizeExpr { size: 1 },
        ] },
        true
    )]
    #[case(
        vec![json!({"status": "Ready"}), json!({"status": "NotReady"})],
        Expr::OrExpr { or: vec![
            Expr::SizeExpr { size: 3 },
            Expr::AllExpr { all: json!({"status": "Ready"}) },
        ] },
        false
    )]
    fn test_assert_expr(
        #[case] input: Vec<serde_json::Value>,
        #[case] expr: Expr,
        #[case] expected: bool,
    ) {
        let v = input.iter().collect::<Vec<&serde_json::Value>>();
        let result = assert_expr(&v, &expr);
        assert_eq!(result.is_ok(), expected);
    }
}

//! Decimal aggregates for cost columns stored as TEXT.
//!
//! Summing those strings with `CAST AS REAL` accumulates binary float error.
//! `decimal_sum` adds them as `Decimal` and converts to `f64` once.
//! `decimal_add` stores a rollup total back as text.

use rusqlite::functions::{Aggregate, Context, FunctionFlags};
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::str::FromStr;

use crate::error::AppError;

struct DecimalSum;

impl Aggregate<Decimal, f64> for DecimalSum {
    fn init(&self, _: &mut Context<'_>) -> rusqlite::Result<Decimal> {
        Ok(Decimal::ZERO)
    }

    fn step(&self, ctx: &mut Context<'_>, sum: &mut Decimal) -> rusqlite::Result<()> {
        *sum = sum
            .checked_add(decimal_value(ctx.get_raw(0)))
            .unwrap_or(Decimal::MAX);
        Ok(())
    }

    fn finalize(&self, _: &mut Context<'_>, sum: Option<Decimal>) -> rusqlite::Result<f64> {
        Ok(sum.unwrap_or(Decimal::ZERO).to_f64().unwrap_or(0.0))
    }
}

pub(crate) fn register_decimal_functions(conn: &Connection) -> Result<(), AppError> {
    conn.create_aggregate_function(
        "decimal_sum",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        DecimalSum,
    )
    .map_err(|error| AppError::Database(format!("注册 decimal_sum 失败: {error}")))?;
    conn.create_scalar_function(
        "decimal_add",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let sum = decimal_value(ctx.get_raw(0))
                .checked_add(decimal_value(ctx.get_raw(1)))
                .unwrap_or(Decimal::MAX);
            Ok(sum.normalize().to_string())
        },
    )
    .map_err(|error| AppError::Database(format!("注册 decimal_add 失败: {error}")))?;
    Ok(())
}

fn decimal_value(value: ValueRef<'_>) -> Decimal {
    match value {
        ValueRef::Null => Decimal::ZERO,
        ValueRef::Integer(number) => Decimal::from(number),
        ValueRef::Real(number) => {
            Decimal::from_str(&format!("{number:.12}")).unwrap_or(Decimal::ZERO)
        }
        ValueRef::Text(bytes) => std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| Decimal::from_str(text.trim()).ok())
            .unwrap_or(Decimal::ZERO),
        ValueRef::Blob(_) => Decimal::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_sum_does_not_drop_small_fractions() {
        let conn = Connection::open_in_memory().expect("memory");
        register_decimal_functions(&conn).expect("register");
        conn.execute_batch(
            "CREATE TABLE costs (amount TEXT);
             INSERT INTO costs VALUES ('0.10');
             INSERT INTO costs VALUES ('0.20');",
        )
        .expect("seed");
        let sum: f64 = conn
            .query_row("SELECT decimal_sum(amount) FROM costs", [], |row| {
                row.get(0)
            })
            .expect("sum");
        assert!((sum - 0.3).abs() < 1e-12);

        let text: String = conn
            .query_row("SELECT decimal_add('0.10', '0.20')", [], |row| row.get(0))
            .expect("add");
        assert_eq!(text, "0.3");
    }
}

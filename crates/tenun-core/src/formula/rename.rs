//! Keeping formulas true when a column is renamed (V4-D25).
//!
//! The text of a formula is the user's. A rename changes only the references to the column
//! and nothing else: spaces, the case of function names and the text between double quotes
//! stay as they were, byte for byte.

use super::lex::{Tok, Token, lex};

/// Rewrites a formula that belongs to `table` after its column `old` was renamed `new`.
/// Both `[old]` and `[table].[old]` are references to that column, so both change.
///
/// `table` is the name the formula would use for the table (its label). A formula can also
/// write the table id; call the function again with the id to catch those.
pub fn rename_column(text: &str, table: &str, old: &str, new: &str) -> String {
    rewrite(text, table, old, new, true)
}

/// Rewrites a formula that does not belong to `table` (a measure with no table, a column of
/// another table) after the column `old` of `table` was renamed `new`. Only `[table].[old]`
/// is a reference to that column here; a bare `[old]` is another table's column.
pub fn rename_column_qualified(text: &str, table: &str, old: &str, new: &str) -> String {
    rewrite(text, table, old, new, false)
}

/// Rewrites a formula after the link `old` was renamed `new` (V4-D28): `[Old].[region]` becomes
/// `[New].[region]`. Only the name of the link changes; the rest of the text stays byte for
/// byte, as in [`rename_column`].
///
/// **Who calls it:** the code that stores formulas, from `Engine::rename_link` (calculated columns
/// in build 02, measures and group values in 03), for every stored formula, in the same step that
/// renames the link. Nothing stores a formula yet, so nothing calls it yet.
pub fn rename_link(text: &str, old: &str, new: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + new.len());
    let mut done = 0;
    for t in lex(text) {
        let Tok::Ref {
            table: Some(qualifier),
            ..
        } = &t.tok
        else {
            continue;
        };
        if !same(qualifier, old) {
            continue;
        }
        let Some(offset) = column_start(&chars[t.at..t.at + t.len]) else {
            continue;
        };
        // The first name in brackets ends one before the `.` that comes before the column.
        out.extend(&chars[done..t.at]);
        out.push_str(&bracketed(new));
        done = t.at + offset - 1;
    }
    out.extend(&chars[done..]);
    out
}

/// The same name, ignoring case and the spaces at the ends: how a formula finds a column.
fn same(a: &str, b: &str) -> bool {
    a.trim() == b.trim() || a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// A name written between brackets. A `]` inside is written twice.
fn bracketed(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// Where the second `[` of `[Table].[Column]` starts, in characters from the start of the
/// token. The first name may hold `]]`.
fn column_start(token: &[char]) -> Option<usize> {
    let mut i = 1;
    while i < token.len() {
        if token[i] == ']' {
            if token.get(i + 1) == Some(&']') {
                i += 2;
                continue;
            }
            // The closing bracket of the table. `.[` follows.
            return (token.get(i + 1) == Some(&'.') && token.get(i + 2) == Some(&'['))
                .then_some(i + 2);
        }
        i += 1;
    }
    None
}

/// `COUNTROWS([Orders])` names a table in brackets. It is not a column.
fn names_a_table(tokens: &[Token], i: usize) -> bool {
    i >= 2
        && tokens[i - 1].tok == Tok::LParen
        && matches!(&tokens[i - 2].tok, Tok::Name(n) if n.eq_ignore_ascii_case("COUNTROWS"))
        && tokens.get(i + 1).is_some_and(|t| t.tok == Tok::RParen)
}

fn rewrite(text: &str, table: &str, old: &str, new: &str, bare: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let tokens = lex(text);
    let mut out = String::with_capacity(text.len() + new.len());
    // Characters before this index are already copied.
    let mut done = 0;
    for (i, t) in tokens.iter().enumerate() {
        let Tok::Ref {
            table: qualifier,
            column,
        } = &t.tok
        else {
            continue;
        };
        if !same(column, old) {
            continue;
        }
        let (from, with) = match qualifier {
            None if bare && !names_a_table(&tokens, i) => (t.at, bracketed(new)),
            Some(q) if same(q, table) => {
                let Some(offset) = column_start(&chars[t.at..t.at + t.len]) else {
                    continue;
                };
                (t.at + offset, bracketed(new))
            }
            _ => continue,
        };
        out.extend(&chars[done..from]);
        out.push_str(&with);
        done = t.at + t.len;
    }
    out.extend(&chars[done..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_reference_and_a_qualified_one_change() {
        assert_eq!(
            rename_column(
                "[amount] * 2 + [Orders].[amount]",
                "Orders",
                "amount",
                "total"
            ),
            "[total] * 2 + [Orders].[total]"
        );
    }

    #[test]
    fn everything_else_stays_byte_for_byte() {
        let text = "  if( [amount]>1 ,  \"[amount] and [Orders].[amount]\" ,Sum(1)) ";
        assert_eq!(
            rename_column(text, "Orders", "amount", "total"),
            "  if( [total]>1 ,  \"[amount] and [Orders].[amount]\" ,Sum(1)) "
        );
        // Nothing to change: the same text comes back.
        assert_eq!(rename_column(text, "Orders", "other", "x"), text);
    }

    #[test]
    fn another_tables_formula_changes_only_the_qualified_reference() {
        let text = "SUM([Orders].[amount]) / SUM([amount])";
        assert_eq!(
            rename_column_qualified(text, "Orders", "amount", "total"),
            "SUM([Orders].[total]) / SUM([amount])"
        );
        assert_eq!(
            rename_column_qualified(text, "Targets", "amount", "total"),
            text
        );
    }

    #[test]
    fn a_reference_to_another_table_with_the_same_column_name_stays() {
        assert_eq!(
            rename_column("[Stores].[amount] + [amount]", "Orders", "amount", "total"),
            "[Stores].[amount] + [total]"
        );
    }

    #[test]
    fn names_match_without_case_and_with_brackets_inside() {
        assert_eq!(
            rename_column("[AMOUNT] + [orders].[Amount]", "Orders", "amount", "a]b"),
            "[a]]b] + [orders].[a]]b]"
        );
        assert_eq!(
            rename_column("[a]]b] + 1", "T", "a]b", "plain"),
            "[plain] + 1"
        );
    }

    #[test]
    fn a_renamed_link_changes_only_its_own_name() {
        assert_eq!(
            rename_link(
                "[Shipped from].[region] & \"[Shipped from].[x]\" & [Sold at].[region] & [region]",
                "Shipped from",
                "Sent from"
            ),
            "[Sent from].[region] & \"[Shipped from].[x]\" & [Sold at].[region] & [region]"
        );
        // Case, spaces and a bracket in the new name.
        assert_eq!(
            rename_link("SUM([ shipped FROM ].[qty])", "Shipped from", "a]b"),
            "SUM([a]]b].[qty])"
        );
        // Nothing to change: the same text comes back.
        let text = "  [Stores].[region] + 1 ";
        assert_eq!(rename_link(text, "Shipped from", "x"), text);
    }

    #[test]
    fn a_table_name_in_countrows_is_not_a_column() {
        assert_eq!(
            rename_column("COUNTROWS([Orders]) + [Orders]", "T", "Orders", "Sales"),
            "COUNTROWS([Orders]) + [Sales]"
        );
    }

    #[test]
    fn accents_and_emoji_survive() {
        assert_eq!(
            rename_column("\"año 😀\" & [niño]", "T", "niño", "anak"),
            "\"año 😀\" & [anak]"
        );
    }

    #[test]
    fn text_after_a_broken_piece_is_left_alone() {
        // The open quote ends the readable part; what comes before it still changes.
        assert_eq!(
            rename_column("[amount] & \"open [amount]", "T", "amount", "x"),
            "[x] & \"open [amount]"
        );
    }
}

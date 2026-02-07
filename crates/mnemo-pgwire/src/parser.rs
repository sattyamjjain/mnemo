//! Minimal SQL parser for pgwire queries.
//!
//! Parses a limited SQL subset and maps to Mnemo operations.
//! This is not a full SQL parser — it handles the common patterns
//! that clients will use to interact with the memories table.

/// Parsed SQL statement mapped to a Mnemo operation.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedStatement {
    /// SELECT query on the memories table.
    Select(SelectQuery),
    /// INSERT into the memories table.
    Insert(InsertQuery),
    /// DELETE from the memories table.
    Delete(DeleteQuery),
    /// Unrecognized or unsupported statement.
    Unsupported(String),
}

/// A parsed SELECT statement.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectQuery {
    /// WHERE agent_id = '...'
    pub agent_id: Option<String>,
    /// WHERE content LIKE '%...%' or free-text query
    pub query_text: Option<String>,
    /// LIMIT clause
    pub limit: usize,
    /// OFFSET clause
    pub offset: usize,
}

/// A parsed INSERT statement.
#[derive(Debug, Clone, PartialEq)]
pub struct InsertQuery {
    pub content: String,
    pub agent_id: Option<String>,
    pub importance: Option<f32>,
    pub memory_type: Option<String>,
    pub tags: Vec<String>,
}

/// A parsed DELETE statement.
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteQuery {
    /// WHERE id = '...'
    pub memory_id: Option<String>,
    /// WHERE agent_id = '...'
    pub agent_id: Option<String>,
}

/// Parse a SQL string into a `ParsedStatement`.
///
/// Supports:
/// - `SELECT * FROM memories [WHERE ...] [LIMIT n] [OFFSET n]`
/// - `INSERT INTO memories (col, ...) VALUES (val, ...)`
/// - `DELETE FROM memories WHERE id = '...'`
pub fn parse_sql(sql: &str) -> ParsedStatement {
    let trimmed = sql.trim().trim_end_matches(';');
    let upper = trimmed.to_uppercase();

    if upper.starts_with("SELECT") {
        parse_select(trimmed)
    } else if upper.starts_with("INSERT") {
        parse_insert(trimmed)
    } else if upper.starts_with("DELETE") {
        parse_delete(trimmed)
    } else {
        ParsedStatement::Unsupported(trimmed.to_string())
    }
}

fn parse_select(sql: &str) -> ParsedStatement {
    let upper = sql.to_uppercase();
    let mut query = SelectQuery {
        agent_id: None,
        query_text: None,
        limit: 50,
        offset: 0,
    };

    // Extract LIMIT
    if let Some(pos) = upper.find("LIMIT") {
        let after = &sql[pos + 5..].trim();
        if let Some(num_str) = after.split_whitespace().next() {
            if let Ok(n) = num_str.parse::<usize>() {
                query.limit = n;
            }
        }
    }

    // Extract OFFSET
    if let Some(pos) = upper.find("OFFSET") {
        let after = &sql[pos + 6..].trim();
        if let Some(num_str) = after.split_whitespace().next() {
            if let Ok(n) = num_str.parse::<usize>() {
                query.offset = n;
            }
        }
    }

    // Extract WHERE agent_id = '...'
    if let Some(agent_id) = extract_string_condition(&upper, sql, "AGENT_ID") {
        query.agent_id = Some(agent_id);
    }

    // Extract WHERE content LIKE '%...%'
    if let Some(pos) = upper.find("CONTENT LIKE") {
        let after = &sql[pos + 12..].trim();
        if let Some(value) = extract_quoted_value(after) {
            // Strip % wildcards
            let clean = value.trim_matches('%').to_string();
            if !clean.is_empty() {
                query.query_text = Some(clean);
            }
        }
    }

    ParsedStatement::Select(query)
}

fn parse_insert(sql: &str) -> ParsedStatement {
    // Extract column names and values from INSERT INTO memories (cols) VALUES (vals)
    let upper = sql.to_uppercase();

    let cols_start = match upper.find('(') {
        Some(p) => p,
        None => return ParsedStatement::Unsupported(sql.to_string()),
    };
    let cols_end = match upper[cols_start..].find(')') {
        Some(p) => cols_start + p,
        None => return ParsedStatement::Unsupported(sql.to_string()),
    };

    let values_marker = match upper[cols_end..].find("VALUES") {
        Some(p) => cols_end + p,
        None => return ParsedStatement::Unsupported(sql.to_string()),
    };

    let vals_start = match upper[values_marker..].find('(') {
        Some(p) => values_marker + p,
        None => return ParsedStatement::Unsupported(sql.to_string()),
    };
    let vals_end = match sql[vals_start..].rfind(')') {
        Some(p) => vals_start + p,
        None => return ParsedStatement::Unsupported(sql.to_string()),
    };

    let columns: Vec<String> = sql[cols_start + 1..cols_end]
        .split(',')
        .map(|c| c.trim().to_uppercase())
        .collect();

    let values: Vec<String> = split_sql_values(&sql[vals_start + 1..vals_end]);

    let mut insert = InsertQuery {
        content: String::new(),
        agent_id: None,
        importance: None,
        memory_type: None,
        tags: vec![],
    };

    for (i, col) in columns.iter().enumerate() {
        if i >= values.len() {
            break;
        }
        let val = unquote(&values[i]);
        match col.as_str() {
            "CONTENT" => insert.content = val,
            "AGENT_ID" => insert.agent_id = Some(val),
            "IMPORTANCE" => insert.importance = val.parse().ok(),
            "MEMORY_TYPE" => insert.memory_type = Some(val),
            _ => {}
        }
    }

    if insert.content.is_empty() {
        return ParsedStatement::Unsupported(sql.to_string());
    }

    ParsedStatement::Insert(insert)
}

fn parse_delete(sql: &str) -> ParsedStatement {
    let upper = sql.to_uppercase();
    let mut delete = DeleteQuery {
        memory_id: None,
        agent_id: None,
    };

    if let Some(id) = extract_string_condition(&upper, sql, "ID") {
        delete.memory_id = Some(id);
    }
    if let Some(agent_id) = extract_string_condition(&upper, sql, "AGENT_ID") {
        delete.agent_id = Some(agent_id);
    }

    ParsedStatement::Delete(delete)
}

/// Extract a string value from `WHERE column = 'value'` pattern.
fn extract_string_condition(upper: &str, original: &str, column: &str) -> Option<String> {
    let pattern = format!("{column} =");
    if let Some(pos) = upper.find(&pattern) {
        let after = &original[pos + pattern.len()..].trim_start();
        return extract_quoted_value(after);
    }
    None
}

/// Extract a single-quoted string value.
fn extract_quoted_value(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with('\'') {
        if let Some(end) = s[1..].find('\'') {
            return Some(s[1..1 + end].to_string());
        }
    }
    None
}

/// Split SQL values, respecting quoted strings.
fn split_sql_values(s: &str) -> Vec<String> {
    let mut values = vec![];
    let mut current = String::new();
    let mut in_quote = false;

    for ch in s.chars() {
        match ch {
            '\'' if !in_quote => {
                in_quote = true;
                current.push(ch);
            }
            '\'' if in_quote => {
                in_quote = false;
                current.push(ch);
            }
            ',' if !in_quote => {
                values.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        values.push(trimmed);
    }
    values
}

/// Remove surrounding quotes from a value string.
fn unquote(s: &str) -> String {
    let trimmed = s.trim();
    if (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || (trimmed.starts_with('"') && trimmed.ends_with('"'))
    {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_select_basic() {
        let stmt = parse_sql("SELECT * FROM memories LIMIT 10");
        match stmt {
            ParsedStatement::Select(q) => {
                assert_eq!(q.limit, 10);
                assert_eq!(q.offset, 0);
                assert!(q.agent_id.is_none());
            }
            other => panic!("Expected Select, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_select_with_where() {
        let stmt = parse_sql("SELECT * FROM memories WHERE agent_id = 'bot-1' LIMIT 5");
        match stmt {
            ParsedStatement::Select(q) => {
                assert_eq!(q.agent_id.as_deref(), Some("bot-1"));
                assert_eq!(q.limit, 5);
            }
            other => panic!("Expected Select, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_select_with_like() {
        let stmt = parse_sql("SELECT * FROM memories WHERE content LIKE '%hello%' LIMIT 20");
        match stmt {
            ParsedStatement::Select(q) => {
                assert_eq!(q.query_text.as_deref(), Some("hello"));
                assert_eq!(q.limit, 20);
            }
            other => panic!("Expected Select, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_insert() {
        let stmt = parse_sql(
            "INSERT INTO memories (content, importance) VALUES ('test memory', 0.8)",
        );
        match stmt {
            ParsedStatement::Insert(q) => {
                assert_eq!(q.content, "test memory");
                assert_eq!(q.importance, Some(0.8));
            }
            other => panic!("Expected Insert, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_insert_with_agent() {
        let stmt = parse_sql(
            "INSERT INTO memories (content, agent_id, memory_type) VALUES ('data', 'agent-1', 'episodic')",
        );
        match stmt {
            ParsedStatement::Insert(q) => {
                assert_eq!(q.content, "data");
                assert_eq!(q.agent_id.as_deref(), Some("agent-1"));
                assert_eq!(q.memory_type.as_deref(), Some("episodic"));
            }
            other => panic!("Expected Insert, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_delete() {
        let stmt =
            parse_sql("DELETE FROM memories WHERE id = '550e8400-e29b-41d4-a716-446655440000'");
        match stmt {
            ParsedStatement::Delete(q) => {
                assert_eq!(
                    q.memory_id.as_deref(),
                    Some("550e8400-e29b-41d4-a716-446655440000")
                );
            }
            other => panic!("Expected Delete, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_unsupported() {
        let stmt = parse_sql("DROP TABLE memories");
        assert!(matches!(stmt, ParsedStatement::Unsupported(_)));
    }

    #[test]
    fn test_parse_select_with_offset() {
        let stmt = parse_sql("SELECT * FROM memories LIMIT 10 OFFSET 20");
        match stmt {
            ParsedStatement::Select(q) => {
                assert_eq!(q.limit, 10);
                assert_eq!(q.offset, 20);
            }
            other => panic!("Expected Select, got {:?}", other),
        }
    }
}

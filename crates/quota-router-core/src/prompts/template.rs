use std::collections::HashMap;
use thiserror::Error;

const MAX_VARIABLE_VALUE_LEN: usize = 10 * 1024; // 10KB

#[derive(Error, Debug)]
pub enum TemplateError {
    #[error("Template variable missing and no default: {0}")]
    VariableMissing(String),
    #[error("Template render error: {0}")]
    RenderError(String),
}

#[derive(Debug, Clone)]
pub enum TemplateFilter {
    Default(String),
    Truncate(usize),
    Upper,
    Lower,
    Strip,
}

pub struct TemplateEngine;

impl TemplateEngine {
    /// Render template with variables using single-pass substitution.
    /// Values containing `{{` are rendered literally (no re-scan).
    pub fn render(
        template: &str,
        variables: &HashMap<String, String>,
        defaults: &HashMap<String, String>,
    ) -> Result<String, TemplateError> {
        let mut result = String::with_capacity(template.len());
        let chars: Vec<char> = template.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            if i + 1 < len && chars[i] == '{' && chars[i + 1] == '{' {
                // Find closing }}
                let start = i + 2;
                let mut end = start;
                while end + 1 < len && !(chars[end] == '}' && chars[end + 1] == '}') {
                    end += 1;
                }
                if end + 1 < len {
                    let var_expr: String = chars[start..end].iter().collect();
                    let (var_name, filters) = parse_variable_expression(&var_expr);
                    let value = resolve_variable(var_name, variables, defaults, &filters)?;
                    // Single-pass: push value literally, don't re-scan
                    result.push_str(&value);
                    i = end + 2;
                } else {
                    // Unclosed {{ — treat as literal
                    result.push(chars[i]);
                    i += 1;
                }
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }

        Ok(result)
    }
}

fn parse_variable_expression(expr: &str) -> (&str, Vec<TemplateFilter>) {
    let parts: Vec<&str> = expr.split('|').map(|s| s.trim()).collect();
    let var_name = parts[0];
    let mut filters = Vec::new();

    for part in &parts[1..] {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("default:") {
            filters.push(TemplateFilter::Default(val.trim().to_string()));
        } else if let Some(val) = part.strip_prefix("truncate:") {
            if let Ok(n) = val.trim().parse::<usize>() {
                filters.push(TemplateFilter::Truncate(n));
            }
        } else if part.eq_ignore_ascii_case("upper") {
            filters.push(TemplateFilter::Upper);
        } else if part.eq_ignore_ascii_case("lower") {
            filters.push(TemplateFilter::Lower);
        } else if part.eq_ignore_ascii_case("strip") {
            filters.push(TemplateFilter::Strip);
        }
    }

    (var_name, filters)
}

fn resolve_variable(
    var_name: &str,
    variables: &HashMap<String, String>,
    defaults: &HashMap<String, String>,
    filters: &[TemplateFilter],
) -> Result<String, TemplateError> {
    let mut value = variables
        .get(var_name)
        .or_else(|| defaults.get(var_name))
        .cloned();

    // Apply Default filter if no value found
    if value.is_none() {
        for f in filters {
            if let TemplateFilter::Default(ref default_val) = f {
                value = Some(default_val.clone());
                break;
            }
        }
    }

    let mut value = value.ok_or_else(|| TemplateError::VariableMissing(var_name.to_string()))?;

    // Sanitize: truncate to max length
    if value.len() > MAX_VARIABLE_VALUE_LEN {
        value.truncate(MAX_VARIABLE_VALUE_LEN);
    }

    // Apply remaining filters
    for f in filters {
        match f {
            TemplateFilter::Default(_) => {} // Already applied
            TemplateFilter::Truncate(n) => {
                let char_count = value.chars().count();
                if char_count > *n {
                    value = value.chars().take(*n).collect();
                }
            }
            TemplateFilter::Upper => {
                value = value.to_uppercase();
            }
            TemplateFilter::Lower => {
                value = value.to_lowercase();
            }
            TemplateFilter::Strip => {
                value = value.trim().to_string();
            }
        }
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_rendering() {
        let template = "Hello {{name}}, your order {{order_id}} is {{status}}.";
        let variables = HashMap::from([
            ("name".to_string(), "John".to_string()),
            ("order_id".to_string(), "12345".to_string()),
            ("status".to_string(), "shipped".to_string()),
        ]);
        let result = TemplateEngine::render(template, &variables, &HashMap::new()).unwrap();
        assert_eq!(result, "Hello John, your order 12345 is shipped.");
    }

    #[test]
    fn test_template_default_filter() {
        let template = "Hello {{name | default:World}}";
        let result = TemplateEngine::render(template, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_template_truncate_filter() {
        let template = "{{bio | truncate:10}}";
        let variables = HashMap::from([(
            "bio".to_string(),
            "This is a very long biography".to_string(),
        )]);
        let result = TemplateEngine::render(template, &variables, &HashMap::new()).unwrap();
        assert_eq!(result, "This is a ");
    }

    #[test]
    fn test_template_upper_filter() {
        let template = "{{name | upper}}";
        let variables = HashMap::from([("name".to_string(), "hello".to_string())]);
        let result = TemplateEngine::render(template, &variables, &HashMap::new()).unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn test_template_lower_filter() {
        let template = "{{name | lower}}";
        let variables = HashMap::from([("name".to_string(), "HELLO".to_string())]);
        let result = TemplateEngine::render(template, &variables, &HashMap::new()).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_template_strip_filter() {
        let template = "{{name | strip}}";
        let variables = HashMap::from([("name".to_string(), "  hello  ".to_string())]);
        let result = TemplateEngine::render(template, &variables, &HashMap::new()).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_single_pass_no_injection() {
        let template = "Hello {{name}}";
        let variables = HashMap::from([("name".to_string(), "{{evil}}".to_string())]);
        let result = TemplateEngine::render(template, &variables, &HashMap::new()).unwrap();
        assert_eq!(result, "Hello {{evil}}");
    }

    #[test]
    fn test_variable_missing_no_default() {
        let template = "Hello {{name}}";
        let result = TemplateEngine::render(template, &HashMap::new(), &HashMap::new());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TemplateError::VariableMissing(_)
        ));
    }

    #[test]
    fn test_defaults_from_defaults_map() {
        let template = "Hello {{name}}";
        let defaults = HashMap::from([("name".to_string(), "World".to_string())]);
        let result = TemplateEngine::render(template, &HashMap::new(), &defaults).unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_variable_takes_precedence_over_default() {
        let template = "Hello {{name | default:World}}";
        let variables = HashMap::from([("name".to_string(), "Alice".to_string())]);
        let result = TemplateEngine::render(template, &variables, &HashMap::new()).unwrap();
        assert_eq!(result, "Hello Alice");
    }

    #[test]
    fn test_multiple_filters() {
        let template = "{{name | strip | upper}}";
        let variables = HashMap::from([("name".to_string(), "  hello  ".to_string())]);
        let result = TemplateEngine::render(template, &variables, &HashMap::new()).unwrap();
        assert_eq!(result, "HELLO");
    }
}

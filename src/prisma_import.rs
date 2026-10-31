//! P1: Prisma → RSM conversion with report
use crate::schema::{parse, format, Diagnostic};

#[derive(Debug, Clone)]
pub struct ImportReport {
    pub warnings: Vec<String>,
    pub translated: Vec<String>,
    pub manual: Vec<String>,
}

pub fn convert_prisma_to_rsm(input: &str) -> Result<(String, ImportReport), Diagnostic> {
    let mut warnings = Vec::new();
    let mut translated = Vec::new();
    let mut manual = Vec::new();

    // Basic checks for previewFeatures (ignored with warning)
    if input.contains("previewFeatures") {
        warnings.push("previewFeatures ignored - not supported in RSM; manual review required".to_string());
    }
    if input.contains("provider") && input.contains("sqlite") {
        warnings.push("SQLite provider not supported in P1; only postgresql is supported".to_string());
    }

    // Simple transformations: replace prisma-client-js with rustorm-client
    let mut transformed = input.replace("prisma-client-js", "rustorm-client");
    // Strip previewFeatures = [...] assignment (keep other props on same line)
    // e.g. generator client { provider = "prisma-client-js" previewFeatures = ["metrics"] }
    let mut cleaned = transformed.clone();
    while let Some(start) = cleaned.find("previewFeatures") {
        // find '=' after
        let eq = cleaned[start..].find('=').map(|i| start + i);
        if let Some(eq_idx) = eq {
            let after_eq = eq_idx + 1;
            if let Some(br_start_rel) = cleaned[after_eq..].find('[') {
                let br_start = after_eq + br_start_rel;
                if let Some(br_end_rel) = cleaned[br_start..].find(']') {
                    let br_end = br_start + br_end_rel + 1;
                    let mut end = br_end;
                    // optional trailing comma and whitespace
                    let tail = &cleaned[end..];
                    let mut chars = tail.chars();
                    // skip whitespace
                    let mut ws_len = 0;
                    for c in chars.clone() {
                        if c.is_whitespace() { ws_len += c.len_utf8(); } else { break; }
                    }
                    end += ws_len;
                    if cleaned[end..].starts_with(',') {
                        end += 1;
                        // skip following whitespace
                        let tail2 = &cleaned[end..];
                        let mut ws2 = 0;
                        for c in tail2.chars() {
                            if c.is_whitespace() { ws2 += c.len_utf8(); } else { break; }
                        }
                        end += ws2;
                    } else if cleaned[start..].chars().rev().take_while(|c| c.is_whitespace() || *c == ',').count() > 0 {
                        // if there was preceding comma/space, also trim leading whitespace of the attribute
                        // backtrack to include preceding whitespace/comma
                        let mut new_start = start;
                        while new_start > 0 && cleaned[..new_start].chars().last().map(|c| c.is_whitespace() || c == ',').unwrap_or(false) {
                            new_start -= 1;
                        }
                        // but keep at least one space if needed; simpler keep original start
                    }
                    cleaned.replace_range(start..end, "");
                    continue;
                }
            }
        }
        break;
    }
    // Also handle provider = "prisma" variants? Keep postgresql same
    // Remove comments that contain @prisma. ? For now preserve as comments
    // Detect unknown attributes: we can try to parse after transformation
    // Try to parse as RSM; if fails, report
    // For attributes like @db. etc., they are already supported via @db

    // Handle datasource url: Prisma may use env("DATABASE_URL") same as RSM, so keep

    // Attempt to parse to validate
    match parse(&cleaned) {
        Ok(schema) => {
            // Successfully parsed, now format
            let formatted = format(&cleaned)?;
            translated.push(format!("Converted {} models, {} enums", schema.models.len(), schema.enums.len()));
            Ok((formatted, ImportReport { warnings, translated, manual }))
        }
        Err(e) => {
            // Try to provide helpful report: find unsupported constructs
            // Look for MongoDB provider etc.
            if input.contains("provider = \"mongodb\"") {
                manual.push("MongoDB provider requires manual conversion".to_string());
            }
            // Check for unsupported types like Decimal without @db? Already handled
            // Check for composite types etc.
            // Return error with diagnostic
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simple_prisma_to_rsm() {
        let prisma = r#"datasource db {
  provider = "postgresql"
  url = env("DATABASE_URL")
}
generator client {
  provider = "prisma-client-js"
}
model User {
  id Int @id @default(autoincrement())
  email String @unique
}
"#;
        let (rsm, report) = convert_prisma_to_rsm(prisma).unwrap();
        assert!(rsm.contains("rustorm-client"));
        assert!(report.warnings.is_empty());
    }
}

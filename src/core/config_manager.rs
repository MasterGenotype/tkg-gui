use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub enum Line {
    Comment(String),
    Assignment { key: String, value: String, raw: String },
    Empty,
}

pub struct ConfigManager {
    lines: Vec<Line>,
    path: std::path::PathBuf,
}

impl ConfigManager {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let re = Regex::new(r#"^(_\w+)\s*=\s*["']?([^"'#\n]*)["']?"#).unwrap();

        let lines: Vec<Line> = content
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    Line::Empty
                } else if trimmed.starts_with('#') {
                    Line::Comment(line.to_string())
                } else if let Some(caps) = re.captures(line) {
                    Line::Assignment {
                        key: caps[1].to_string(),
                        value: caps[2].trim().to_string(),
                        raw: line.to_string(),
                    }
                } else {
                    Line::Comment(line.to_string())
                }
            })
            .collect();

        Ok(Self { lines, path })
    }

    pub fn get_option(&self, key: &str) -> Option<String> {
        for line in &self.lines {
            if let Line::Assignment { key: k, value, .. } = line {
                if k == key {
                    return Some(value.clone());
                }
            }
        }
        None
    }

    pub fn set_option(&mut self, key: &str, value: &str) {
        for line in &mut self.lines {
            if let Line::Assignment {
                key: k,
                value: v,
                raw,
            } = line
            {
                if k == key {
                    *v = value.to_string();
                    *raw = format!("{}=\"{}\"", k, value);
                    return;
                }
            }
        }
        // If not found, add it
        self.lines.push(Line::Assignment {
            key: key.to_string(),
            value: value.to_string(),
            raw: format!("{}=\"{}\"", key, value),
        });
    }

    pub fn get_all_options(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for line in &self.lines {
            if let Line::Assignment { key, value, .. } = line {
                map.insert(key.clone(), value.clone());
            }
        }
        map
    }

    pub fn save(&self) -> Result<(), String> {
        let content: String = self
            .lines
            .iter()
            .map(|line| match line {
                Line::Comment(s) => s.clone(),
                Line::Assignment { raw, .. } => raw.clone(),
                Line::Empty => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");

        fs::write(&self.path, content + "\n").map_err(|e| e.to_string())
    }
}

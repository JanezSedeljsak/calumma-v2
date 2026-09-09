use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct Catalog {
    strings: HashMap<String, String>,
}

impl Catalog {
    pub fn load(language: &str, root: &Path) -> Result<Self> {
        let path = root.join("translations").join(format!("{language}.json"));
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading locale file {}", path.display()))?;
        let strings: HashMap<String, String> =
            serde_json::from_str(&text).context("parsing locale JSON")?;
        Ok(Self { strings })
    }

    pub fn get(&self, key: &str) -> String {
        self.strings
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    pub fn format(&self, key: &str, args: &[&str]) -> String {
        let mut out = self.get(key);
        for (i, arg) in args.iter().enumerate() {
            out = out.replace(&format!("{{{}}}", i), arg);
        }
        out
    }
}

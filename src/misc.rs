use std::collections::HashMap;
use std::str::FromStr;

pub struct IdNameMap(pub HashMap<u64, String>);

impl IdNameMap {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn search(&self, query: &str) -> Vec<(u64, String)> {
        match u64::from_str(query) {
            Ok(x) => match self.0.get(&x) {
                Some(y) => vec![(x, y.clone())],
                None => vec![],
            },
            Err(_) => self
                .0
                .iter()
                .filter_map(|(x, y)| {
                    y.to_lowercase()
                        .contains(&query.to_lowercase())
                        .then(|| (*x, String::clone(y)))
                })
                .collect(),
        }
    }

    pub fn lookup<F>(&self, query: &str, c: F) -> String
    where
        F: FnOnce(u64, &str) -> String,
    {
        let matches = self.search(&query);
        match matches.len() {
            0 => format!("no matches for `{}` found", query),
            1 => c(matches[0].0, &matches[0].1),
            _ => {
                let mut res = format!(
                    "`{}` is ambiguous. perhaps you meant one of these:\n",
                    query
                );
                res.push_str("```\n");
                res.push_str(
                    &matches
                        .iter()
                        .take(5)
                        .map(|(x, y)| format!("{} {}", x, y))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                res.push_str("```");
                res
            }
        }
    }
}

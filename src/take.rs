pub struct Take {
    pub name: String,
    pub start_sample: u64,
    pub end_sample: u64,
}

impl Take {
    pub fn new(name: &str, start_sample: u64, end_sample: u64) -> Take {
        Take {
            name: name.to_string(),
            start_sample,
            end_sample,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}

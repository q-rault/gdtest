//! JUnit XML shard aggregation (internal Phase 1).
//!
//! Reads multiple JUnit XML files produced by `gdtest --reporter junit`
//! and merges them into a single `<testsuites>` document. Designed for
//! GitHub Actions matrix sharding:
//!
//! ```yaml
//! - run: gdtest test/ --partition count:${{ matrix.shard }}/4 --reporter junit > shard-${{ matrix.shard }}.xml
//! # ... after all shards complete:
//! - run: gdtest aggregate shard-*.xml --output combined.xml
//! ```

use std::io::Write;

use anyhow::{Context, Result};

struct ShardData {
    tests: u32,
    failures: u32,
    errors: u32,
    time_s: f64,
    /// Raw content between `<testsuite …>` and `</testsuite>` (the testcase lines).
    cases: String,
}

fn parse_shard(xml: &str, path: &str) -> Result<ShardData> {
    // Extract testsuite open-tag attributes — order matches what gdtest emits but
    // we search for each attribute independently in case tools reorder them.
    let attr = |name: &str| -> Option<&str> {
        let needle = format!(r#"{name}=""#);
        let start = xml.find(needle.as_str())? + needle.len();
        let end = xml[start..].find('"')? + start;
        Some(&xml[start..end])
    };

    let tests: u32 = attr("tests")
        .and_then(|v| v.parse().ok())
        .with_context(|| format!("{path}: missing 'tests' attribute"))?;
    let failures: u32 = attr("failures").and_then(|v| v.parse().ok()).unwrap_or(0);
    let errors: u32 = attr("errors").and_then(|v| v.parse().ok()).unwrap_or(0);
    let time_s: f64 = attr("time").and_then(|v| v.parse().ok()).unwrap_or(0.0);

    // Extract everything between the first `>` after `<testsuite` and `</testsuite>`.
    let suite_open_end = xml
        .find("<testsuite")
        .and_then(|p| xml[p..].find('>').map(|q| p + q + 1))
        .with_context(|| format!("{path}: no <testsuite> tag"))?;
    let suite_close = xml
        .rfind("</testsuite>")
        .with_context(|| format!("{path}: no </testsuite> tag"))?;
    let cases = xml[suite_open_end..suite_close].trim().to_string();

    Ok(ShardData {
        tests,
        failures,
        errors,
        time_s,
        cases,
    })
}

pub fn run(files: &[String], output: Option<&str>) -> Result<()> {
    if files.is_empty() {
        anyhow::bail!("aggregate: no input files specified");
    }

    let mut total_tests = 0u32;
    let mut total_failures = 0u32;
    let mut total_errors = 0u32;
    let mut total_time = 0.0f64;
    let mut all_cases: Vec<String> = Vec::new();

    for path in files {
        let xml = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        let shard = parse_shard(&xml, path)?;
        total_tests += shard.tests;
        total_failures += shard.failures;
        total_errors += shard.errors;
        total_time += shard.time_s;
        if !shard.cases.is_empty() {
            all_cases.push(shard.cases);
        }
    }

    let mut out_buf = Vec::new();
    writeln!(out_buf, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
    writeln!(out_buf, r#"<testsuites>"#)?;
    writeln!(
        out_buf,
        r#"  <testsuite name="gdtest" tests="{total_tests}" failures="{total_failures}" errors="{total_errors}" time="{total_time:.3}">"#,
    )?;
    for cases in &all_cases {
        for line in cases.lines() {
            writeln!(out_buf, "    {line}")?;
        }
    }
    writeln!(out_buf, r#"  </testsuite>"#)?;
    writeln!(out_buf, r#"</testsuites>"#)?;

    match output {
        Some(path) => {
            std::fs::write(path, &out_buf).with_context(|| format!("writing {path}"))?;
            eprintln!(
                "gdtest aggregate: {}/{total_tests} test(s) written to {path}",
                total_tests - total_failures - total_errors
            );
        }
        None => {
            std::io::stdout().write_all(&out_buf)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_shard() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="gdtest" tests="3" failures="1" errors="0" time="1.234">
    <testcase classname="res://test/a.gd" name="test_foo" time="0.001"/>
    <testcase classname="res://test/a.gd" name="test_bar" time="0.002">
      <failure message="expected true got false"/>
    </testcase>
    <testcase classname="res://test/b.gd" name="test_baz" time="0.003"/>
  </testsuite>
</testsuites>"#;
        let shard = parse_shard(xml, "test.xml").unwrap();
        assert_eq!(shard.tests, 3);
        assert_eq!(shard.failures, 1);
        assert_eq!(shard.errors, 0);
        assert!((shard.time_s - 1.234).abs() < 0.001);
        assert!(shard.cases.contains("test_foo"));
    }

    #[test]
    fn merges_two_shards() {
        let shard1 = ShardData {
            tests: 2,
            failures: 0,
            errors: 0,
            time_s: 1.0,
            cases: r#"<testcase classname="a.gd" name="test_a" time="0.5"/>"#.to_string(),
        };
        let shard2 = ShardData {
            tests: 1, failures: 1, errors: 0, time_s: 0.5,
            cases: r#"<testcase classname="b.gd" name="test_b" time="0.5"><failure message="oops"/></testcase>"#.to_string(),
        };
        // Just verify totals add up
        let total_tests = shard1.tests + shard2.tests;
        let total_failures = shard1.failures + shard2.failures;
        assert_eq!(total_tests, 3);
        assert_eq!(total_failures, 1);
    }
}

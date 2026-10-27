//! Bounded deterministic replay fuzz for the forms untrusted-byte parsers.
//!
//! Scope: `list_fields` (`NotAPdf` / `XFA` paths) and `fill_and_flatten` with
//! caller-supplied font bytes. Layout-math fuzzing is out of scope.
//!
//! Determinism: every case derives from the checked-in `tests/seeds` corpus
//! through a fixed xorshift generator. No network, no threads, no sleeps, no
//! wall-clock input, and no shared `HashMap` iteration. The same checkout
//! produces the same cases in the same order on every run.
//!
//! Bounds: exactly `1 + MUTATIONS_PER_SEED` cases per seed, each case no
//! longer than its seed (`MAX_SEED_BYTES` caps every seed). The run panics
//! past `TIME_BUDGET` naming the case that exceeded it, so a hung parser
//! fails instead of hanging CI.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pdfcraft_forms::{
    fill_and_flatten, list_fields, ErrorKind, FieldValue, FillOptions, FontBytes,
};

/// Exact corpus. Removing or adding a seed fails the replay test with a
/// corpus-mismatch message instead of silently shrinking coverage.
const EXPECTED_SEEDS: &[&str] = &[
    "f01-basic-form.pdf",
    "f02-xfa-form.pdf",
    "f03-empty.bin",
    "f04-not-a-pdf.bin",
    "f05-truncated-pdf.bin",
    "n01-not-a-font.bin",
    "n02-ttcf-magic.bin",
    "n03-empty-font.bin",
];

/// Mutated cases per seed, excluding the unmutated base case. Fixed, so the
/// total case count is a pure function of the corpus.
const MUTATIONS_PER_SEED: usize = 32;

/// Largest seed the harness accepts. Mutants never exceed their seed length.
const MAX_SEED_BYTES: usize = 65_536;

/// Wall-clock hang detector, not a schedule: tiny inputs finish in
/// milliseconds, so exceeding this means a parser stopped making progress.
const TIME_BUDGET: Duration = Duration::from_secs(120);

/// Fixed generator seed. The stream depends only on `(LCG_SEED, mutation)`.
const LCG_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// Deterministic xorshift64* step.
fn next_rand(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// Index into `0..bound` from the deterministic stream. Callers only pass a
/// positive `bound` no larger than a seed length, so both conversions hold.
fn below(state: &mut u64, bound: usize) -> usize {
    let wide = u64::try_from(bound).expect("seed length fits in u64");
    usize::try_from(next_rand(state) % wide).expect("masked index fits in usize")
}

/// Deterministic bounded mutation of `bytes` for case `mutation`.
///
/// Output length never exceeds input length (one byte when empty), so memory
/// stays bounded without any runtime cap.
#[must_use]
fn mutate(bytes: &[u8], mutation: usize) -> Vec<u8> {
    if bytes.is_empty() {
        return vec![u8::try_from(mutation & 0xFF).expect("masked mutation index fits in u8")];
    }
    let mut out = bytes.to_vec();
    let mut state = LCG_SEED ^ ((mutation as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    if state == 0 {
        state = 0x243F_6A88_85A3_08D3;
    }
    let ops = 1 + below(&mut state, 3);
    for _ in 0..ops {
        let op = below(&mut state, 4);
        let pos = below(&mut state, out.len());
        match op {
            0 => out[pos] ^= 1 << below(&mut state, 8),
            1 => {
                out[pos] =
                    u8::try_from(next_rand(&mut state) & 0xFF).expect("masked byte fits in u8");
            }
            2 => {
                let other = below(&mut state, out.len());
                out.swap(pos, other);
            }
            _ => {
                let cut = 1 + below(&mut state, out.len());
                out.truncate(cut.min(out.len()));
            }
        }
    }
    out
}

fn seeds_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/seeds")
}

fn read_seed(name: &str) -> Vec<u8> {
    fs::read(seeds_dir().join(name))
        .unwrap_or_else(|error| panic!("read fuzz seed {name}: {error}"))
}

fn text_field() -> HashMap<String, FieldValue> {
    HashMap::from([(
        "employee_name".to_string(),
        FieldValue::Text("Ada".to_string()),
    )])
}

fn check_budget(started: &Instant, case: &str) {
    assert!(
        started.elapsed() <= TIME_BUDGET,
        "fuzz time budget exceeded at case {case}"
    );
}

#[test]
fn replay_seed_corpus() {
    let started = Instant::now();
    let mut names: Vec<String> = fs::read_dir(seeds_dir())
        .expect("list fuzz seeds")
        .map(|entry| {
            entry
                .expect("read fuzz seed entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    let expected: Vec<String> = EXPECTED_SEEDS.iter().map(ToString::to_string).collect();
    assert_eq!(
        names, expected,
        "seed corpus mismatch: harness covers exactly EXPECTED_SEEDS"
    );

    let form_pdf = read_seed("f01-basic-form.pdf");
    let mut cases = 0_usize;
    for name in &names {
        let seed = read_seed(name);
        assert!(
            seed.len() <= MAX_SEED_BYTES,
            "seed {name} exceeds MAX_SEED_BYTES"
        );
        let is_font_seed = name.starts_with('n');
        for mutation in 0..=MUTATIONS_PER_SEED {
            let input = if mutation == 0 {
                seed.clone()
            } else {
                mutate(&seed, mutation)
            };
            let case = format!("{name}#{mutation}");
            eprintln!("fuzz case {case} ({} bytes)", input.len());
            if is_font_seed {
                let options = FillOptions {
                    font: Some(FontBytes {
                        bytes: &input,
                        index: 0,
                    }),
                    ..FillOptions::default()
                };
                let result = fill_and_flatten(&form_pdf, &text_field(), &options);
                let rejected = matches!(
                    result,
                    Err(ref error)
                        if matches!(error.kind, ErrorKind::FontNotConfigured { .. })
                );
                assert!(
                    rejected,
                    "fuzz case {case}: garbage font bytes must stay FontNotConfigured, got {result:?}"
                );
            } else {
                let listed = list_fields(&input);
                eprintln!("fuzz case {case}: list_fields -> {}", listed.is_ok());
                if name == "f01-basic-form.pdf" && mutation == 0 {
                    let mut checkbox = HashMap::new();
                    checkbox.insert("agree".to_string(), FieldValue::CheckBox(true));
                    let filled = fill_and_flatten(&input, &checkbox, &FillOptions::default())
                        .expect("fill base");
                    assert!(filled.starts_with(b"%PDF"), "fuzz case {case}: fill output");
                }
            }
            cases += 1;
            check_budget(&started, &case);
        }
    }
    eprintln!("fuzz done: {cases} cases in {:?}", started.elapsed());
}

/// A deterministically corrupted header must be rejected, proving the
/// harness can tell a caught input from a missed one.
#[test]
fn mutated_seed_is_rejected() {
    let mut corrupted = read_seed("f01-basic-form.pdf");
    for byte in corrupted.iter_mut().take(8) {
        *byte = 0;
    }
    let err = list_fields(&corrupted).expect_err("f01-basic-form.pdf with zeroed header must fail");
    assert_eq!(err.kind, ErrorKind::NotAPdf, "zeroed header input");

    let mut font = read_seed("n01-not-a-font.bin");
    font.extend_from_slice(b"fuzz");
    let options = FillOptions {
        font: Some(FontBytes {
            bytes: &font,
            index: 0,
        }),
        ..FillOptions::default()
    };
    let err = fill_and_flatten(&read_seed("f01-basic-form.pdf"), &text_field(), &options)
        .expect_err("n01-not-a-font.bin mutant must fail");
    assert!(
        matches!(err.kind, ErrorKind::FontNotConfigured { .. }),
        "mutated font input, got {:?}",
        err.kind
    );
}

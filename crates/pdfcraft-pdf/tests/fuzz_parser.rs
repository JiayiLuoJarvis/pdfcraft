//! Bounded deterministic replay fuzz for the pdf untrusted-byte parser.
//!
//! Scope: PNG pixel bytes through `Resources::add_image` plus `layout_spec`,
//! which is where PNG pixel data fails. Font bytes need a caller-supplied
//! face (no in-repo font), and layout-math fuzzing is out of scope.
//!
//! Determinism: every case derives from the checked-in `tests/seeds` corpus
//! through a fixed xorshift generator. No network, no threads, no sleeps, no
//! wall-clock input. The same checkout produces the same cases in the same
//! order on every run.
//!
//! Bounds: exactly `1 + MUTATIONS_PER_SEED` cases per seed, each case no
//! longer than its seed (`MAX_SEED_BYTES` caps every seed). The run panics
//! past `TIME_BUDGET` naming the case that exceeded it, so a hung parser
//! fails instead of hanging CI.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pdfcraft_pdf::spec::{Alignment, Block, ImageSpec, Resources, SectionSpec};
use pdfcraft_pdf::{layout_spec, DocumentSpec, ImageFormat, PageSize};

/// Exact corpus. Removing or adding a seed fails the replay test with a
/// corpus-mismatch message instead of silently shrinking coverage.
const EXPECTED_SEEDS: &[&str] = &[
    "g01-one-pixel.png",
    "g02-bad-idat.png",
    "g03-empty.bin",
    "g04-not-a-png.bin",
    "g05-truncated-png.bin",
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
const LCG_SEED: u64 = 0xB529_7A4D_682E_6AF3;

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

/// Image-only document: no text, so no caller font is needed and the PNG
/// bytes are the only untrusted input.
fn lay_out(png: Vec<u8>) -> pdfcraft_pdf::Result<pdfcraft_pdf::Rendered> {
    let mut resources: Resources<()> = Resources::default();
    let image = resources.add_image(png, ImageFormat::Png);
    layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            body: vec![Block::Image(ImageSpec {
                image,
                width: None,
                height: None,
                align: Alignment::Start,
            })],
            ..SectionSpec::default()
        }],
    })
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

    let mut cases = 0_usize;
    for name in &names {
        let seed = read_seed(name);
        assert!(
            seed.len() <= MAX_SEED_BYTES,
            "seed {name} exceeds MAX_SEED_BYTES"
        );
        for mutation in 0..=MUTATIONS_PER_SEED {
            let input = if mutation == 0 {
                seed.clone()
            } else {
                mutate(&seed, mutation)
            };
            let case = format!("{name}#{mutation}");
            eprintln!("fuzz case {case} ({} bytes)", input.len());
            match lay_out(input) {
                Ok(_) => eprintln!("fuzz case {case}: layout ok"),
                Err(errors) => eprintln!("fuzz case {case}: layout -> {errors}"),
            }
            cases += 1;
            check_budget(&started, &case);
        }
    }
    eprintln!("fuzz done: {cases} cases in {:?}", started.elapsed());
}

/// A deterministically corrupted PNG must be rejected, proving the harness
/// can tell a caught input from a missed one.
#[test]
fn mutated_seed_is_rejected() {
    let mut corrupted = read_seed("g01-one-pixel.png");
    for byte in corrupted.iter_mut().take(8) {
        *byte = 0;
    }
    lay_out(corrupted).expect_err("g01-one-pixel.png with zeroed signature must fail");
}

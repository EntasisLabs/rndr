//! Seeded edit sequences. A failure names the seed and the step.
//!
//! The CI run is a few hundred steps on one seed. The longer soak is ignored:
//!
//! ```text
//! RNDR_FUZZ_SEED=1 RNDR_FUZZ_FRAMES=20000 \
//!   cargo test -p rndr_markdown --test fuzz -- --ignored fuzz_soak --exact
//! ```

use rndr_markdown::{apply_text, apply_view, parse, Command, Session};

const CI_SEED: u64 = 0xC0FF_EE00;
const CI_STEPS: u64 = 200;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(0x5DEECE66D).wrapping_add(0xB);
        self.0
    }

    fn gen(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() as usize) % n
        }
    }
}

fn run(seed: u64, steps: u64) {
    let mut rng = Rng(seed);
    let mut bytes = sample(&mut rng);
    let mut tree = parse(&bytes);
    assert_eq!(tree.concat(), bytes, "seed {seed} open");
    let opened = Session::open(&bytes);
    assert_eq!(opened.journal_frames(), 0, "seed {seed} open frames");
    assert_eq!(opened.bytes(), bytes);
    for step in 0..steps {
        let cmd = command(&mut rng, &bytes);
        let applied = match apply_view(&bytes, &tree, &cmd) {
            Ok(applied) => applied,
            Err(_) => continue,
        };
        let via_text = apply_text(&bytes, &applied.splices);
        let full = parse(&via_text);
        assert_eq!(
            full, applied.tree,
            "seed {seed} step {step} cmd {cmd:?}\nbefore {bytes:?}\nafter {via_text:?}"
        );
        assert_eq!(applied.tree.concat(), via_text);
        bytes = via_text;
        tree = applied.tree;
    }
}

fn sample(rng: &mut Rng) -> String {
    const DOCS: &[&str] = &[
        "alpha\n\nbeta\n\ngamma\n",
        "say _a_ please\n",
        "Foo\n---\n\nbar\n",
        "- a\nb\n\nnext\n",
        "- a\n\n- b\n\nnext\n",
        "<div>\n\nfoo\n",
        "<script>\nfoo\n\nbar\n</script>\n\nafter\n",
        "[foo]: /url\n\n[foo]\n\nmore\n",
        "```callout\ntone: tip\n# keep\nbody: Hi\n```\n\nafter\n",
        "$$\nx\n$$\n\nplain\n",
        "",
        "é\n",
    ];
    DOCS[rng.gen(DOCS.len())].to_string()
}

fn command(rng: &mut Rng, bytes: &str) -> Command {
    let at = boundary(bytes, rng.gen(bytes.len().saturating_add(1)));
    match rng.gen(5) {
        0 => Command::Insert {
            at,
            text: ["Z", " ", "*", "_", "\n", "é"][rng.gen(6)].to_string(),
        },
        1 => {
            let end = boundary(bytes, at + rng.gen(4));
            Command::Delete {
                start: at.min(end),
                end: at.max(end).min(bytes.len()),
            }
        }
        2 => {
            let end = boundary(bytes, at + 1 + rng.gen(3));
            Command::ToggleEmphasis {
                start: at.min(end),
                end: at.max(end).min(bytes.len()),
            }
        }
        3 => Command::Split { at },
        _ => Command::Insert {
            at,
            text: "q".into(),
        },
    }
}

fn boundary(bytes: &str, index: usize) -> usize {
    let index = index.min(bytes.len());
    if bytes.is_char_boundary(index) {
        index
    } else {
        let mut at = index;
        while at > 0 && !bytes.is_char_boundary(at) {
            at -= 1;
        }
        at
    }
}

#[test]
fn fuzz_ci() {
    run(CI_SEED, CI_STEPS);
}

#[test]
#[ignore]
fn fuzz_soak() {
    let seed = std::env::var("RNDR_FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(CI_SEED);
    let steps = std::env::var("RNDR_FUZZ_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000);
    run(seed, steps);
}

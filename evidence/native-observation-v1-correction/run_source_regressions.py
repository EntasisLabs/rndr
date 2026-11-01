"""Exact observer/paint source compilation plus red/green regression replay.
Reuses the sealed review compiler recipe/dependency paths, NOT its verdict.
Existing pinned dependency rlibs (including Cargo-rebuilt core) are reused.
Writes scratch only under target/native-observation-correction. No repo mutation.
"""
from pathlib import Path
import subprocess
import hashlib

recipe = Path('evidence/native-observation-v1-review/run_rust_probes.py').read_text()
recipe = recipe.replace("target/native-observation-review", "target/native-observation-correction")
recipe = recipe.replace("Path('crates/rndr_render/src/observe_tests.rs').read_text()+extra",
                        "Path('crates/rndr_render/src/observe_tests.rs').read_text().replace('rndr_paint::', 'crate::fresh_paint::')")
# The new permanent fixtures carry the same actual-paint/Render-swap reproduction,
# same-kind replacement, epoch mutation, builtin updates and unchanged-path tests.
print('CURRENT EXACT OBSERVER + PAINT SOURCE', flush=True)
exec(compile(recipe, '<sealed recipe with permanent correction tests>', 'exec'))
print('ACTUAL_CURRENT_COMPILED_OBSERVER', hashlib.sha256(Path('target/native-observation-correction/observe.rs').read_bytes()).hexdigest(), flush=True)

# Retest three permanent regressions against the original observer. Current core
# is deliberately retained: the baseline observer does not check even its epoch.
# The unavailable paint-kind accessor is only a compilation scaffold in the
# unchanged-path test, which is NOT run in this baseline red replay.
baseline = recipe.replace("source=Path('crates/rndr_render/src/observe.rs').read_text()",
    "source=subprocess.check_output(['git','show','28a0ae307f869ef651d8ebcc5fdcd82a9dc31ce7:crates/rndr_render/src/observe.rs'],text=True)")
baseline = baseline.replace("Path('crates/rndr_render/src/observe_tests.rs').read_text()",
    "Path('crates/rndr_render/src/observe_tests.rs').read_text().replace('let kind = capture.text_engine_kind();', 'let kind = \"baseline compile scaffold, not executed\";')")
baseline = baseline.replace("subprocess.run([str(out/'probe-tests'),'--nocapture','--test-threads=2'],check=True)", "")
print('BASELINE OBSERVER (current core retained)', flush=True)
exec(compile(baseline, '<baseline observer red replay>', 'exec'))
print('ACTUAL_BASELINE_COMPILED_OBSERVER', hashlib.sha256(Path('target/native-observation-correction/observe.rs').read_bytes()).hexdigest(), flush=True)
for test in ['correction_render_engine_replacements_are_denied_even_same_kind',
             'correction_custom_epoch_change_in_render_is_denied',
             'correction_builtin_mutations_in_render_are_denied']:
    result = subprocess.run(['target/native-observation-correction/probe-tests', test,
                             '--nocapture', '--test-threads=1'])
    print('EXPECTED RED', test, 'exit', result.returncode, flush=True)
    if result.returncode != 101:
        raise RuntimeError('baseline regression did not fail as expected')

"""Check every production Rust crate for off-scale color, radius and spacing recipes.

Tests may use synthetic values. A non-recipe operation (e.g. decoding an OS
color) may use a same-line `// design-lint: <reason>` exception. There is no
historical baseline: additions and existing code follow the same rules.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
# Mask strings and comments before matching across lines; preserve offsets.
LEX = re.compile(r'//[^\n]*|/\*.*?\*/|(?:b?r)(?P<hash>\#*)".*?"(?P=hash)|b?"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\\n])\'', re.S)
TEST = re.compile(r'#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s+)?mod\s+\w+\s*\{')
RULE = re.compile(r'\b(?:Color\s*::\s*rgba?|UiColor\s*::\s*Explicit|hex)\s*\(|'
                  r'\bShadow\s*::\s*new\s*\(|\bRadius\s*\(\s*\d|'
                  r'\b(?:\w+_)?(?:radius|gap|padding|space)\s*[:=]\s*(?:Some\s*\(\s*)?\d')


def violations(source):
    reasons = set()
    def blank(match):
        if re.fullmatch(r'//\s*design-lint:\s*\S.*', match[0]):
            line = source.count('\n', 0, match.start()) + 1
            reasons.add(line)
            # rustfmt places a call's trailing explanation on its first argument line.
            before = source[:match.start()].splitlines()
            if len(before) >= 2 and not before[-1].strip() and before[-2].rstrip().endswith('('):
                reasons.add(line - 1)
        return re.sub(r'[^\n]', ' ', match[0])
    code = LEX.sub(blank, source)
    # Only cfg(test) module bodies, not everything below the first test.
    test_ranges = []
    for test in TEST.finditer(code):
        depth = 1
        for brace in re.finditer('[{}]', code[test.end():]):
            depth += 1 if brace[0] == '{' else -1
            if depth == 0:
                test_ranges.append((test.start(), test.end() + brace.end()))
                break
    for match in RULE.finditer(code):
        line = code.count('\n', 0, match.start()) + 1
        if line not in reasons and not any(a <= match.start() < b for a, b in test_ranges):
            yield line, ' '.join(match[0].split())


def main():
    errors = []
    for path in sorted((ROOT/'crates').glob('*/src/**/*.rs')):
        if path.name in {'style.rs', 'tokens.rs', 'tests.rs'} or 'tests' in path.parts:
            continue
        errors.extend(f'{path.relative_to(ROOT)}:{line}: {rule}' for line, rule in violations(path.read_text()))
    if errors:
        sys.exit('Use a semantic token, or explain a non-recipe operation on its line:\n' + '\n'.join(errors))


if __name__ == '__main__':
    main()

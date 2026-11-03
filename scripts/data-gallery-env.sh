# Source from the repository root (bash or zsh). Keep an explicitly supplied
# compatible target cache; never clean, copy or silently redirect another cache.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export PYTHONDONTWRITEBYTECODE=1

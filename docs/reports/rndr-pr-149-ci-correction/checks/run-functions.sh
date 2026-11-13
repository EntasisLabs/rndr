failed=0
run() {
    label=$1; shift
    free=$(df -k . | awk 'NR==2 {print $4}')
    # Leave at least 3 GiB free. Do not clean shared or historical artifacts.
    test "$free" -ge 3145728 || { echo "disk blocker before $label"; exit 74; }
    { printf '$'; printf ' %s' "$@"; printf '\n'; } > "$out/$label.txt"
    "$@" >> "$out/$label.txt" 2>&1 & pid=$!
    (
        while kill -0 "$pid" 2>/dev/null; do
            now=$(df -k . | awk 'NR==2 {print $4}')
            if [ "$now" -lt 3145728 ]; then
                echo "disk blocker: $now KiB" > "$out/disk-blocker.txt"
                kill -INT "$pid" 2>/dev/null || true
                exit
            fi
            sleep 2
        done
    ) & monitor=$!
    code=0; wait "$pid" || code=$?
    kill "$monitor" 2>/dev/null || true; wait "$monitor" 2>/dev/null || true
    after=$(df -k . | awk 'NR==2 {print $4}')
    printf '\nexit=%s\n' "$code" >> "$out/$label.txt"
    printf '%s\t%s\t%s\t%s\n' "$label" "$code" "$free" "$after" >> "$out/summary.tsv"
    echo "$label exit=$code free_kib=$after"
    test ! -e "$out/disk-blocker.txt" || exit 74
    # Keep exploring subsequent configurations, but fail the overall driver.
    if [ "$code" -ne 0 ]; then failed=1; fi
}

#!/bin/bash
# Simple memory monitoring using ps
# Monitors memory usage of cargo test process

set -e

TEST_NAME=${1:-"test_100k_vectors_save_load_search"}
OUTPUT_FILE="target/memory_profile_${TEST_NAME}_$(date +%Y%m%d_%H%M%S).txt"

echo "Memory Profiling: ${TEST_NAME}"
echo "Output: ${OUTPUT_FILE}"
echo ""

# Run test in background and capture PID
cargo test --release --test integration_chunked_tests ${TEST_NAME} -- --ignored --nocapture > ${OUTPUT_FILE} 2>&1 &
TEST_PID=$!

echo "Test PID: ${TEST_PID}"
echo "Monitoring memory usage (sampling every 0.5s)..."
echo ""

# Monitor memory
PEAK_RSS=0
PEAK_VSZ=0
SAMPLE_COUNT=0

echo "Time,RSS_KB,VSZ_KB" > ${OUTPUT_FILE}.mem_samples.csv

while kill -0 ${TEST_PID} 2>/dev/null; do
    # Get memory stats from ps (RSS in KB, VSZ in KB)
    MEM_STATS=$(ps -p ${TEST_PID} -o rss=,vsz= 2>/dev/null || echo "0 0")
    RSS=$(echo $MEM_STATS | awk '{print $1}')
    VSZ=$(echo $MEM_STATS | awk '{print $2}')

    if [ "$RSS" != "0" ] && [ "$RSS" != "" ]; then
        TIMESTAMP=$(date +%s.%N)
        echo "${TIMESTAMP},${RSS},${VSZ}" >> ${OUTPUT_FILE}.mem_samples.csv

        # Track peak
        if [ $RSS -gt $PEAK_RSS ]; then
            PEAK_RSS=$RSS
        fi
        if [ $VSZ -gt $PEAK_VSZ ]; then
            PEAK_VSZ=$VSZ
        fi

        SAMPLE_COUNT=$((SAMPLE_COUNT + 1))

        # Print progress
        RSS_MB=$((RSS / 1024))
        echo -ne "\rCurrent: ${RSS_MB} MB | Peak: $((PEAK_RSS / 1024)) MB | Samples: ${SAMPLE_COUNT}   "
    fi

    sleep 0.5
done

echo ""
echo ""
wait ${TEST_PID}
TEST_EXIT_CODE=$?

echo "=========================================="
echo "Memory Profile Results"
echo "=========================================="
echo ""
echo "Test: ${TEST_NAME}"
echo "Exit Code: ${TEST_EXIT_CODE}"
echo "Samples Collected: ${SAMPLE_COUNT}"
echo ""
echo "Peak RSS (Resident Set Size): $((PEAK_RSS / 1024)) MB ($PEAK_RSS KB)"
echo "Peak VSZ (Virtual Size): $((PEAK_VSZ / 1024)) MB ($PEAK_VSZ KB)"
echo ""

# Check against target
TARGET_MB=200
PEAK_RSS_MB=$((PEAK_RSS / 1024))

if [ $PEAK_RSS_MB -lt $TARGET_MB ]; then
    echo "✓ Memory usage WITHIN target (<${TARGET_MB} MB)"
else
    echo "✗ Memory usage EXCEEDS target (>${TARGET_MB} MB)"
fi

echo ""
echo "Full test output: ${OUTPUT_FILE}"
echo "Memory samples: ${OUTPUT_FILE}.mem_samples.csv"
echo ""

exit $TEST_EXIT_CODE

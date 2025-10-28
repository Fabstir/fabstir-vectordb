#!/bin/bash
# Memory profiling script for chunked storage implementation
# Profiles memory usage during load, search, and cache operations

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PROFILE_DIR="target/memory_profiles"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_FILE="${PROFILE_DIR}/memory_profile_${TIMESTAMP}.txt"

# Create profile directory
mkdir -p "${PROFILE_DIR}"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Memory Profiling for Chunked Storage${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo "Results will be saved to: ${RESULTS_FILE}"
echo ""

# Function to run test with memory profiling
profile_test() {
    local test_name=$1
    local test_binary=$2

    echo -e "${YELLOW}Profiling: ${test_name}${NC}"
    echo "-----------------------------------" | tee -a "${RESULTS_FILE}"
    echo "Test: ${test_name}" | tee -a "${RESULTS_FILE}"
    echo "Time: $(date)" | tee -a "${RESULTS_FILE}"
    echo "" | tee -a "${RESULTS_FILE}"

    # Use /usr/bin/time to get detailed memory statistics
    /usr/bin/time -v cargo test --release ${test_binary} -- --ignored --nocapture 2>&1 | \
        tee -a "${RESULTS_FILE}" | \
        grep -E "(Maximum resident set size|Average resident set size|Page size|Major|Minor|Voluntary|Involuntary|File system|Socket)" || true

    echo "" | tee -a "${RESULTS_FILE}"
}

# Function to extract memory stats
extract_memory_stats() {
    local test_name=$1
    local log_file="${RESULTS_FILE}"

    echo -e "${GREEN}Memory Statistics for ${test_name}:${NC}"

    # Extract key metrics
    local max_rss=$(grep "Maximum resident set size" "${log_file}" | tail -1 | awk '{print $6}')
    if [ -n "$max_rss" ]; then
        local max_rss_mb=$((max_rss / 1024))
        echo "  Peak Memory (RSS): ${max_rss_mb} MB (${max_rss} KB)"

        # Check against target (<200 MB for 10 chunks)
        if [ ${max_rss_mb} -lt 200 ]; then
            echo -e "  ${GREEN}✓ Within target (<200 MB)${NC}"
        else
            echo -e "  ${RED}✗ Exceeds target (>200 MB)${NC}"
        fi
    fi

    echo ""
}

# Main profiling workflow

echo -e "${BLUE}[1/3] Building release binary...${NC}"
cargo build --release --tests 2>&1 | tail -5
echo ""

echo -e "${BLUE}[2/3] Running memory profiles...${NC}"
echo ""

# Profile 1: 100K vectors - Full workflow (load, search)
echo -e "${YELLOW}Profile 1: 100K Vectors - Load & Search${NC}" | tee -a "${RESULTS_FILE}"
profile_test "100K vectors load and search" "test_100k_vectors_save_load_search"

# Extract and display stats
extract_memory_stats "100K Vectors"

# Profile 2: Check if valgrind/massif is available for heap profiling
if command -v valgrind &> /dev/null; then
    echo -e "${BLUE}[3/3] Optional: Heap profiling with Valgrind Massif...${NC}"
    echo "This may take several minutes..."
    echo ""

    MASSIF_OUT="target/memory_profiles/massif.out.${TIMESTAMP}"

    # Run a smaller test with massif
    valgrind --tool=massif \
        --massif-out-file="${MASSIF_OUT}" \
        --time-unit=B \
        --detailed-freq=1 \
        --max-snapshots=100 \
        cargo test --release test_100k_vectors_save_load_search -- --ignored --nocapture \
        2>&1 | tee -a "${RESULTS_FILE}" || true

    if [ -f "${MASSIF_OUT}" ]; then
        echo "" | tee -a "${RESULTS_FILE}"
        echo "Massif heap profile saved to: ${MASSIF_OUT}" | tee -a "${RESULTS_FILE}"
        echo "View with: ms_print ${MASSIF_OUT}" | tee -a "${RESULTS_FILE}"

        # Try to extract peak heap usage
        if command -v ms_print &> /dev/null; then
            echo "" | tee -a "${RESULTS_FILE}"
            echo "Peak heap usage:" | tee -a "${RESULTS_FILE}"
            ms_print "${MASSIF_OUT}" | head -50 | grep -E "(KB|MB|peak)" | tee -a "${RESULTS_FILE}"
        fi
    fi
else
    echo -e "${YELLOW}[3/3] Valgrind not available, skipping heap profiling${NC}"
    echo "To enable: sudo apt-get install valgrind"
fi

echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Memory Profiling Complete${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo "Full results saved to: ${RESULTS_FILE}"
echo ""

# Summary
echo -e "${GREEN}Summary:${NC}"
echo "- Peak memory measured with /usr/bin/time"
echo "- Target: <200 MB for 10 chunks (100K vectors)"
echo "- Check results above for pass/fail status"
echo ""

# Parse and display final summary
echo -e "${BLUE}Final Memory Report:${NC}"
grep -E "(Peak Memory|Within target|Exceeds target)" "${RESULTS_FILE}" || echo "Run complete - check ${RESULTS_FILE} for details"

exit 0

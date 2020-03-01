#!/bin/sh
cd $(dirname "$0")

HFUZZ_RUN_ARGS="\
$HFUZZ_RUN_ARGS\
--threads=4 \
--linux_perf_instr \
--linux_perf_branch \
--max_file_size=32 \
--timeout=1"

cargo hfuzz run fuzz --color=always
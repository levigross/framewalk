# Makefile — Build crash test programs for framewalk.
#
# Two build tiers:
#   build/debug/      — -O0 (full debug info, no optimizations)
#   build/optimized/  — -O2 (optimized: inlining, reordering, elided vars)
#
# Source paths in DWARF are remapped to a nonexistent directory so GDB
# has symbols but `list` cannot show source — the LLM must debug blind.
#
# Usage:
#   make              # build debug + optimized (C/C++ only)
#   make rust         # build Rust programs (requires nix develop)
#   make clean        # remove build directory
#
# Rust programs require the nix devshell toolchain:
#   nix develop --command make rust

BUILD_DIR  ?= build
DBG_DIR    := $(BUILD_DIR)/debug
OPT_DIR    := $(BUILD_DIR)/optimized
SRC_DIR    := $(CURDIR)

CC         ?= gcc
CXX        ?= g++
RUSTC      ?= rustc

# Source path remapping — GDB sees symbols but can't show source.
NOSRC      := /framewalk-challenges/src
MAP_FLAG   := -fdebug-prefix-map=$(SRC_DIR)=$(NOSRC)
RUST_MAP   := --remap-path-prefix=$(SRC_DIR)=$(NOSRC)

# Suppress NixOS's gcc wrapper injecting _FORTIFY_SOURCE (conflicts with -O0).
COMMON     := -g -Wno-cpp $(MAP_FLAG)
DBG_CFLAGS := $(COMMON) -O0
OPT_CFLAGS := $(COMMON) -O2
DBG_CXXFL  := $(COMMON) -O0
OPT_CXXFL  := $(COMMON) -O2
RUSTFLAGS  := -g $(RUST_MAP)
RUST_OPT   := -g -C opt-level=2 $(RUST_MAP)

LDFLAGS_PTHREAD := -pthread

# ── Program lists ───────────────────────────────────────────────────

C_SIMPLE := stack-overflow use-after-free double-free sigfpe sigbus \
            deep-call-chain fork-crash signal-handler-crash \
            function-pointer-crash syscall-torture

C_THREADED := deadlock multithread-crash

CXX_SOURCES := vtable-corruption

RUST_SOURCES := rust-panic rust-unsafe-crash rust-stack-overflow

# ── Aggregate targets ───────────────────────────────────────────────

DBG_C   := $(C_SIMPLE:%=$(DBG_DIR)/%) $(C_THREADED:%=$(DBG_DIR)/%)
DBG_CXX := $(CXX_SOURCES:%=$(DBG_DIR)/%)
DBG_RS  := $(RUST_SOURCES:%=$(DBG_DIR)/%)

OPT_C   := $(C_SIMPLE:%=$(OPT_DIR)/%) $(C_THREADED:%=$(OPT_DIR)/%)
OPT_CXX := $(CXX_SOURCES:%=$(OPT_DIR)/%)
OPT_RS  := $(RUST_SOURCES:%=$(OPT_DIR)/%)

NATIVE  := $(DBG_C) $(DBG_CXX) $(OPT_C) $(OPT_CXX)
RUST    := $(DBG_RS) $(OPT_RS)

.PHONY: all rust clean

all: $(NATIVE)
	@echo "Built $(words $(NATIVE)) programs (debug + optimized)"
	@echo "  debug:     $(DBG_DIR)/"
	@echo "  optimized: $(OPT_DIR)/"
	@echo "Source paths remapped to $(NOSRC)"
	@echo ""
	@echo "For Rust programs: nix develop --command make rust"

rust: $(RUST)
	@echo "Built $(words $(RUST)) Rust programs (debug + optimized)"

clean:
	rm -rf $(BUILD_DIR)

# ── Directory creation ──────────────────────────────────────────────

$(DBG_DIR) $(OPT_DIR):
	mkdir -p $@

# ── Debug tier (-O0) ────────────────────────────────────────────────

# Simple C
$(DBG_DIR)/%: %.c | $(DBG_DIR)
	$(CC) $(DBG_CFLAGS) -o $@ $<

# Threaded C
$(DBG_DIR)/deadlock: deadlock.c | $(DBG_DIR)
	$(CC) $(DBG_CFLAGS) $(LDFLAGS_PTHREAD) -o $@ $<

$(DBG_DIR)/multithread-crash: multithread-crash.c | $(DBG_DIR)
	$(CC) $(DBG_CFLAGS) $(LDFLAGS_PTHREAD) -o $@ $<

# syscall-torture (intentional overflow warning suppressed)
$(DBG_DIR)/syscall-torture: syscall-torture.c | $(DBG_DIR)
	$(CC) $(DBG_CFLAGS) -Wno-stringop-overflow -o $@ $<

# C++
$(DBG_DIR)/vtable-corruption: vtable-corruption.cpp | $(DBG_DIR)
	$(CXX) $(DBG_CXXFL) -Wno-dynamic-class-memaccess -o $@ $<

# Rust
$(DBG_DIR)/rust-%: rust-%.rs | $(DBG_DIR)
	$(RUSTC) $(RUSTFLAGS) -o $@ $<

# ── Optimized tier (-O2) ────────────────────────────────────────────

# Simple C
$(OPT_DIR)/%: %.c | $(OPT_DIR)
	$(CC) $(OPT_CFLAGS) -o $@ $<

# Threaded C
$(OPT_DIR)/deadlock: deadlock.c | $(OPT_DIR)
	$(CC) $(OPT_CFLAGS) $(LDFLAGS_PTHREAD) -o $@ $<

$(OPT_DIR)/multithread-crash: multithread-crash.c | $(OPT_DIR)
	$(CC) $(OPT_CFLAGS) $(LDFLAGS_PTHREAD) -o $@ $<

# syscall-torture
$(OPT_DIR)/syscall-torture: syscall-torture.c | $(OPT_DIR)
	$(CC) $(OPT_CFLAGS) -Wno-stringop-overflow -o $@ $<

# C++
$(OPT_DIR)/vtable-corruption: vtable-corruption.cpp | $(OPT_DIR)
	$(CXX) $(OPT_CXXFL) -Wno-dynamic-class-memaccess -o $@ $<

# Rust
$(OPT_DIR)/rust-%: rust-%.rs | $(OPT_DIR)
	$(RUSTC) $(RUST_OPT) -o $@ $<

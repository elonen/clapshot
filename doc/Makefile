# Variables
SOURCES = $(wildcard *.msc)
TARGETS = $(patsubst %.msc,generated/%.svg,$(SOURCES))

# Phony target to avoid conflicts with files of the same name and to improve performance
.PHONY: all clean mscgen

# Default target
all: mscgen

# Rule to create SVG files from MSC files
mscgen: $(TARGETS)

# Pattern rule to convert MSC to SVG
generated/%.svg: %.msc | generated
	mscgen -T svg -i $< -o $@

# Directory creation rule
generated:
	mkdir -p generated

# Clean up the output directory
clean:
	echo "Nothing to do"

# Central i18n pipeline for Clapshot (server / organizer / client).
#
# One catalog, gettext semantics: msgid = the English SOURCE string, msgctxt disambiguates.
# `en` is the source language (no PO needed; missing translation falls back to the msgid).
#
#   make extract   # harvest msgids from all 3 codebases -> templates/*.pot -> messages.pot
#   make update    # merge messages.pot into each translatable locale's po/<loc>.po
#   make compile   # build runtime catalogs into each component (client JSON, *.mo for py/rust)
#   make stats     # translation coverage per locale
#   make all       # extract + update + compile
#
# Translatable locales (en is the source, so it is NOT listed here):
LOCALES := fi fr zh
DOMAIN  := clapshot

ROOT    := $(abspath ..)
TPL     := templates
PO      := po
NODE    := node

CLIENT_SRC := $(ROOT)/client/src
ORG_DIR    := $(ROOT)/organizer/basic_folders/organizer
SERVER_MAIN := $(ROOT)/server/src/lib.rs

# Compiled-catalog destinations inside each component (produced by `make compile`):
#   - client:    JSON, bundled at build (committed)
#   - organizer: .mo, loaded at runtime via gettext (gitignored, built/deployed)
#   - server:    NO runtime artifact — server/build.rs compiles po/*.po into the binary directly
CLIENT_LOCALES := $(ROOT)/client/src/i18n/locales
ORG_LOCALE     := $(ROOT)/organizer/basic_folders/organizer/locale

.PHONY: all extract update compile stats clean

all: extract update compile

# ---- 1. EXTRACT: per-language harvest into one messages.pot --------------------------------
extract: $(TPL)/messages.pot

$(TPL)/client.pot: FORCE
	@mkdir -p $(TPL)
	cd $(ROOT) && $(NODE) client/scripts/i18n-extract.mjs client/src > $(abspath $(TPL))/client.pot

$(TPL)/organizer.pot: FORCE
	@mkdir -p $(TPL)
	cd $(ROOT) && xgettext --language=Python --from-code=UTF-8 --force-po --sort-by-file \
		--add-comments=TRANSLATORS \
		--keyword=_ --keyword=N_ \
		--keyword=pgettext:1c,2 --keyword=npgettext:1c,2,3 --keyword=ngettext:1,2 \
		-o $(abspath $(TPL))/organizer.pot \
		$$(find organizer/basic_folders/organizer -name '*.py' -not -path '*/testing_methods/*' -not -name 'test_*.py')

$(TPL)/server.pot: FORCE
	@mkdir -p $(TPL)
	xtr --omit-header -o $(TPL)/server.pot \
		-k "tr_user:2" -k "tr_user_fmt:2" -k "tr:2" -k "tr_fmt:2" \
		$(SERVER_MAIN)

$(TPL)/messages.pot: $(TPL)/client.pot $(TPL)/organizer.pot $(TPL)/server.pot
	msgcat --to-code=UTF-8 --sort-by-file $^ -o $@
	@echo "Merged template: $@ ($$(grep -c '^msgid ' $@) entries incl. header)"

# ---- 2. UPDATE: merge template into each locale's PO (create if missing) --------------------
update: $(TPL)/messages.pot
	@mkdir -p $(PO)
	@for loc in $(LOCALES); do \
		if [ -f $(PO)/$$loc.po ]; then \
			echo "msgmerge $(PO)/$$loc.po"; \
			msgmerge --update --backup=none --quiet $(PO)/$$loc.po $(TPL)/messages.pot; \
		else \
			echo "msginit $(PO)/$$loc.po"; \
			msginit --no-translator --locale=$$loc --input=$(TPL)/messages.pot --output-file=$(PO)/$$loc.po; \
		fi; \
	done

# ---- 3. COMPILE: runtime catalogs into each component (server compiles via its own build.rs) -
compile: compile-client compile-organizer

compile-client:
	@mkdir -p $(CLIENT_LOCALES)
	@for loc in $(LOCALES); do \
		if [ -f $(PO)/$$loc.po ]; then echo "client $$loc.json"; \
			$(NODE) $(ROOT)/client/scripts/i18n-po2json.mjs $(PO)/$$loc.po > $(CLIENT_LOCALES)/$$loc.json; fi; \
	done

compile-organizer:
	@for loc in $(LOCALES); do \
		[ -f $(PO)/$$loc.po ] && { mkdir -p $(ORG_LOCALE)/$$loc/LC_MESSAGES; \
			echo "organizer $$loc/$(DOMAIN).mo"; \
			msgfmt $(PO)/$$loc.po -o $(ORG_LOCALE)/$$loc/LC_MESSAGES/$(DOMAIN).mo; }; \
	done

stats:
	@for loc in $(LOCALES); do [ -f $(PO)/$$loc.po ] && { printf "%s: " $$loc; msgfmt --statistics $(PO)/$$loc.po -o /dev/null; }; done

clean:
	rm -rf $(TPL)

FORCE:

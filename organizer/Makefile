SUBDIRS := $(wildcard */.)

.PHONY: subdirs $(SUBDIRS) clean distclean

subdirs: $(SUBDIRS)

$(SUBDIRS):
	$(MAKE) -C $@

distclean:
	for dir in $(SUBDIRS); do \
		cd $$dir && $(MAKE) distclean; \
	done

clean: distclean

debian-docker:
	set -e; \
	for dir in $(SUBDIRS); do \
		$(MAKE) --no-print-directory -C $$dir debian-docker; \
	done

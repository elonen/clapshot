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
	for dir in $(SUBDIRS); do \
		cd $$dir && $(MAKE) debian-docker; \
	done

.PHONY: debian-docker debian-local clean

PACKAGE     = clapshot-htadmin
PVER        = 0.11.0
DEBIAN_VER ?= bookworm
UID         = $(shell id -u)
GID         = $(shell id -g)

DEPS = $(shell find additional_files/ debian/ -type f)

dist_deb/built.$(DEBIAN_VER).docker: $(DEPS)
	mkdir -p dist_deb
	docker run --rm \
		--mount type=bind,source="$$(pwd)",target="/build" \
		--mount type=bind,source="$$(pwd)/dist_deb",target="/dist_deb" \
		--user root \
		debian:$(DEBIAN_VER)-slim \
		bash -c "\
			apt-get -qy update >/dev/null 2>&1 && \
			apt-get -qy install debhelper devscripts >/dev/null 2>&1 && \
			cd /build && \
			dpkg-buildpackage -us -uc -b && \
			cp /$(PACKAGE)_*.deb /dist_deb/ && \
			chown -R $(UID):$(GID) /dist_deb"
	@for f in dist_deb/*.deb; do \
		case "$$f" in *_$(DEBIAN_VER)_*) continue ;; esac; \
		newname=$$(echo "$$f" | sed -E 's/(_[0-9.]+-?[0-9]*_)/\1$(DEBIAN_VER)_/'); \
		[ "$$f" != "$$newname" ] && mv "$$f" "$$newname" || true; \
	done
	touch $@

debian-docker: dist_deb/built.$(DEBIAN_VER).docker

clean:
	rm -rf dist_deb

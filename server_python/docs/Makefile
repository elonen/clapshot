# Minimal makefile for Sphinx documentation
#

# You can set these variables from the command line, and also
# from the environment for the first two.
SPHINXOPTS    ?=
SPHINXBUILD   ?= sphinx-build
SOURCEDIR     = .
BUILDDIR      = _build

PYTHON        = python3.9
ENV           = _venv
IN_ENV        = . ${ENV}/bin/activate;
PYTHON_ENV    = ${ENV}/bin/${PYTHON}


# Put it first so that "make" without argument is like "make help".
help: ${ENV}
	@${IN_ENV} $(SPHINXBUILD) -M help "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) $(O)

.PHONY: help Makefile proper

${ENV}:
	@echo "Creating Python environment '$@'..." >&2
	${PYTHON} -m venv "$@"
	@echo "Updating pip..." >&2
	${PYTHON_ENV} -m pip install -r requirements.txt
	${PYTHON_ENV} -m pip install -r ../requirements.txt
	@echo "=== Virtual env '${ENV}' ready."

# Catch-all target: route all unknown targets to Sphinx using the new
# "make mode" option.  $(O) is meant as a shortcut for $(SPHINXOPTS).
%: ${ENV}  Makefile
	@${IN_ENV} $(SPHINXBUILD) -M $@ "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) $(O)

proper:
	rm -rf _build
	rm -rf ${ENV}

# COPR ("make srpm" build method): makes the source package of Serafind.
# COPR runs: make -f .copr/Makefile srpm outdir=<folder> spec=<spec>

# The version of Cargo.toml: the release tag is checked against it, so the files exist.
version := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

srpm:
	dnf -y install rpm-build rpmdevtools
	mkdir -p $(outdir)
	sed 's/^Version:.*/Version:        $(version)/' packaging/fedora/serafind.spec > $(outdir)/serafind.spec
	spectool --get-files --directory $(outdir) $(outdir)/serafind.spec
	rpmbuild -bs --define "_sourcedir $(outdir)" --define "_srcrpmdir $(outdir)" $(outdir)/serafind.spec

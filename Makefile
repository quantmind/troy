.PHONY: help
help:
	@echo ======================================================================================
	@fgrep -h "##" $(MAKEFILE_LIST) | fgrep -v fgrep | sed -e 's/\\$$//' | sed -e 's/##//'
	@echo ======================================================================================

.PHONY: bench
bench:			## run the criterion benchmarks
	@cargo bench --bench arithmetic
	@cargo bench --bench orderbook

.PHONY: bench-report
bench-report:		## open criterion's own report: violin, PDF and per-parameter charts
	@python3 -c "import webbrowser; webbrowser.open('file://$(PWD)/target/criterion/report/index.html')"

.PHONY: bench-save
bench-save:		## run the benchmarks and commit the numbers to docs/bench-data.json
	@.dev/bench-report save --run

.PHONY: bench-table
bench-table:		## print the benchmark snapshot as a table
	@.dev/bench-report table

.PHONY: docs
docs:			## build the rust documentation
	@cargo doc --all-features --no-deps

.PHONY: docs-open
docs-open:		## build the rust documentation and open it in the browser
	@cargo doc --all-features --no-deps --open

.PHONY: release
release:		## tag current version (from Cargo.toml) and push
	$(eval VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'))
	@read -p "Tagging with v$(VERSION), are you sure? [Y/n] " ans; \
	ans=$${ans:-Y}; \
	if [ "$$ans" = "Y" ] || [ "$$ans" = "y" ]; then \
		git tag -a v$(VERSION) -m "v$(VERSION)" && git push origin v$(VERSION); \
	else \
		echo "Aborted."; \
	fi

.PHONY: rs-fuzz
rs-fuzz:		## check the property tests over a far wider sweep, in release
	@PROPTEST_CASES=$${PROPTEST_CASES:-100000} cargo test --release --test fuzz

.PHONY: rs-lint
rs-lint:		## Lint rust code
	@.dev/rs-lint fix

.PHONY: rs-oracle
rs-oracle:		## check mul and div against the 256-bit reference, full sweep
	@cargo test --release --test oracle

.PHONY: rs-publish
rs-publish:		## publish the crate to crates.io
	@cargo publish --token $(CARGO_REGISTRY_TOKEN)

.PHONY: rs-test
rs-test:		## Run rust tests with all features
	@cargo test --all-features

.PHONY: site
site:			## build the benchmark and docs site into site/dist
	@[ -d site/node_modules ] || npm --prefix site install
	@npm --prefix site run build

.PHONY: site-dev
site-dev:		## serve the site locally with hot reload
	@[ -d site/node_modules ] || npm --prefix site install
	@npm --prefix site run dev

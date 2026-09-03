.PHONY: help
help:
	@echo ======================================================================================
	@fgrep -h "##" $(MAKEFILE_LIST) | fgrep -v fgrep | sed -e 's/\\$$//' | sed -e 's/##//'
	@echo ======================================================================================


.PHONY: rs-lint
rs-lint:		## Lint rust code
	@.dev/rs-lint fix

.PHONY: rs-test
rs-test:		## Run rust tests with all features
	@cargo test --all-features

.PHONY: rs-publish
rs-publish:		## publish the crate to crates.io
	@cargo publish --token $(CARGO_REGISTRY_TOKEN)

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

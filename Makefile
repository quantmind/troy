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

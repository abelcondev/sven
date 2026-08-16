# grok-sdd — build the SDD engine binary.
#
#   make build      build ./engine → ./dist/grok-sdd for this machine
#   make test       run the vendored engine tests
#   make crosscompile  build darwin/linux amd64+arm64 into ./dist
#   make bundle     package skills + hooks → ./dist/grok-sdd-assets.tar.gz
#   make release-assets  crosscompile + bundle (everything a GitHub release needs)
#   make clean

ENGINE := engine
DIST   := dist
BIN    := grok-sdd

.PHONY: build test crosscompile bundle release-assets clean

build:
	cd $(ENGINE) && go build -o ../$(DIST)/$(BIN) .
	@echo "built $(DIST)/$(BIN)"

test:
	cd $(ENGINE) && go test ./...

crosscompile:
	@mkdir -p $(DIST)
	cd $(ENGINE) && GOOS=darwin  GOARCH=arm64 go build -o ../$(DIST)/$(BIN)-darwin-arm64 .
	cd $(ENGINE) && GOOS=darwin  GOARCH=amd64 go build -o ../$(DIST)/$(BIN)-darwin-amd64 .
	cd $(ENGINE) && GOOS=linux   GOARCH=arm64 go build -o ../$(DIST)/$(BIN)-linux-arm64 .
	cd $(ENGINE) && GOOS=linux   GOARCH=amd64 go build -o ../$(DIST)/$(BIN)-linux-amd64 .
	@echo "cross-compiled into $(DIST)/"

bundle:
	@mkdir -p $(DIST)/_stage/hooks
	cp -R grok/skills $(DIST)/_stage/skills
	cp hooks/sdd.json $(DIST)/_stage/hooks/sdd.json
	cp grok/config.example.toml $(DIST)/_stage/config.example.toml
	tar czf $(DIST)/$(BIN)-assets.tar.gz -C $(DIST)/_stage skills hooks config.example.toml
	rm -rf $(DIST)/_stage
	@echo "bundled $(DIST)/$(BIN)-assets.tar.gz"

release-assets: crosscompile bundle
	@echo "release assets ready in $(DIST)/"

clean:
	rm -rf $(DIST)

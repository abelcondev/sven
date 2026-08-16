# grok-sdd — build the SDD engine binary.
#
#   make build      build ./engine → ./dist/grok-sdd for this machine
#   make test       run the vendored engine tests
#   make crosscompile  build darwin/linux amd64+arm64 into ./dist
#   make clean

ENGINE := engine
DIST   := dist
BIN    := grok-sdd

.PHONY: build test crosscompile clean

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

clean:
	rm -rf $(DIST)

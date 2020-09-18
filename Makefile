GO ?= go

mnotify:
	$(GO) build $(GOFLAGS) -o $@ .

update:
	$(GO) get -u .
	$(GO) mod tidy

.PHONY: mnotify update

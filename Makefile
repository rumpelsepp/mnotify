GO ?= go

mnotify:
	$(GO) build $(GOFLAGS) -o $@ .

.PHONY: mnotify

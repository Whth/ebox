
DISPATCH_DIR:=

all:dispatch









release:
	cargo build --release

dispatch: release
	cp target/release/*.exe "$(DISPATCH_DIR)"
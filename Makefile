
DISPATCH_DIR:=

all:dispatch









release:
	cargo build --release -F basic,video

dispatch: release
	cp target/release/*.exe "$(DISPATCH_DIR)"
bin:
	mkdir -p bin
	touch bin

bin/init: bin src/*go
	cd src && CGO_ENABLED=0 go build -ldflags '-w' .
	mv src/extreme-cgi-bin $@

bin/vmm: bin vm_runner/src/*.rs
	cd vm_runner && cargo build --release
	cp vm_runner/target/release/example $@

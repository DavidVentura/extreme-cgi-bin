
bin:
	mkdir -p bin
	touch bin

bin/init: bin src/*go
	cd src && CGO_ENABLED=0 go build -ldflags '-w' .
	mv src/extreme-cgi-bin $@
	touch $@

bin/vmm: bin vm_runner/src/*.rs
	cd vm_runner && cargo build --release
	cp vm_runner/target/release/example $@

artifacts/rootfs.ext4: bin/init
	mkdir -p artifacts/mount
	sudo mount $@ artifacts/mount
	sudo cp -t artifacts/mount $^
	sudo umount artifacts/mount
	touch $@

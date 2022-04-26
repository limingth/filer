all:
	make c
	make s

c client:
	cargo build --release --features client-gui
	cp ./target/release/filer client
rc runclient:
	make c
	./client

s server:
	cargo build --release --features server-gui
	cp ./target/release/filer server
rs runserver:
	make s
	./server
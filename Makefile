.PHONY: dev build test smoke icons clean

# Run Ebb from source with live-reloading UI.
dev:
	npx tauri dev

# Installers for this OS in src-tauri/target/release/bundle/.
build:
	npx tauri build

test:
	cargo test --manifest-path src-tauri/Cargo.toml

# Runs the built app through a sped-up work → break → work cycle (~70 s).
smoke: build
	scripts/smoke-test.sh src-tauri/target/release/ebb

icons:
	npx tauri icon Resources/AppIcon.png -o src-tauri/icons

clean:
	rm -rf build src-tauri/target

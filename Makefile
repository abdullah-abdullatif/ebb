.PHONY: app run test install clean

app:
	scripts/build-app.sh

run: app
	open build/Ebb.app

test:
	swift test

# Launch-at-login (SMAppService) works best from /Applications.
install: app
	rm -rf /Applications/Ebb.app
	cp -R build/Ebb.app /Applications/
	open /Applications/Ebb.app

clean:
	rm -rf build .build

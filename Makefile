.PHONY: app zip run test install clean

app:
	scripts/build-app.sh

# Shareable zip; ditto keeps the code signature intact.
zip: app
	rm -f build/Ebb.zip
	ditto -c -k --keepParent build/Ebb.app build/Ebb.zip

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

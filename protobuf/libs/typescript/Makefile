DIST_DIR := dist

# Build the library
build: $(DIST_DIR)/clapshot.d.ts

# Compile the Typescript code
$(DIST_DIR)/clapshot.d.ts: src/clapshot.ts
	npm install
	./node_modules/.bin/tsc -p tsconfig.json

src/clapshot.ts: ../../proto/*.proto
	npm install
	protoc --experimental_allow_proto3_optional \
		--ts_proto_opt=useExactTypes=false\
		--ts_proto_opt=initializeFieldsAsUndefined=false\
		--ts_proto_opt=unknownFields=true\
		--ts_proto_opt=usePrototypeForDefaults=true\
		--ts_proto_opt=esModuleInterop=true\
		--ts_proto_opt=exportCommonSymbols=false\
		--plugin=./node_modules/.bin/protoc-gen-ts_proto \
		--ts_proto_out=./src \
		--proto_path ../../proto/ \
		clapshot.proto

# Clean the build artifacts
clean:
	rm -rf $(DIST_DIR)
	rm -f src/clapshot.ts
	rm -rf src/google

.PHONY: build clean

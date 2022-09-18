.PHONY: deps install clean build_clean dist_clean tests dist docker

PACKAGE_VERSION=$(shell (git rev-parse --short HEAD || date +"%Y-%m-%d" || echo "dev"))
PACKAGE=clapshot_client

default:
	@echo "No make default. Maybe try 'dev-docker' or 'build-docker'?"



clean:
	rm -rf node_modules dist

npm-deps:
	npm install vite
	npm install

docker: clean 
	docker build -t ${PACKAGE}:${PACKAGE_VERSION} .
	docker tag ${PACKAGE}:${PACKAGE_VERSION} ${PACKAGE}:latest



build-local: npm-deps
	npm run build
	ls	

build-docker: docker
	mkdir -p dist
	docker run --rm -it --mount type=bind,source="$$(pwd)"/dist,target=/build/dist  ${PACKAGE}:latest  make build-local
	@echo "==== Build in Docker done. Results are in ./dist/ ===="

build:
	@echo "Run either 'build-local' or 'build-docker'"



dev-local: npm-deps
	npm run dev -- --host --port 5173

dev-docker: docker
	docker run --rm -it -p 127.0.0.1:5173:5173  ${PACKAGE}:latest

dev:
	@echo "Run either 'dev-local' or 'dev-docker'"


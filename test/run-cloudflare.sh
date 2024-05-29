#!/bin/bash
set -e

# Local directory to bind mount to clapshot_host container. Modify if needed.
LOCAL_DATA_DIR="$(pwd)/CLAPSHOT_CLOUDFLARE_VOLUME"

NETWORK_NAME="clapshot_cloudflare"
CLOUDFLARED_CONTAINER="cloudflared_for_clapshot"
CLAPSHOT_HOST_CONTAINER="clapshot_host"

CLAPSHOT_DOCKER_IMAGE="elonen/clapshot:latest-demo-htadmin"
CLOUDFLARED_DOCKER_IMAGE="cloudflare/cloudflared:latest"


case "$(uname -sr)" in
     CYGWIN*|MINGW*|MINGW32*|MSYS*)
         echo "COMPATIBILITY WARNING: Running on a 'Unix lite' Windows shell. Bind mount on NTFS might cause issue with SQLite, symlinks etc. Use WSL2 instead."
         echo " "
         ;;
esac

echo "--- SECURITY WARNING ---"
echo "This will expose your local dir '$LOCAL_DATA_DIR' to the Internet via Clapshot server and Cloudflare tunnel."
echo " "
echo "It will start containers IN THE BACKGROUND."
echo "Simply closing this terminal will NOT stop the containers. You must use Docker commands to stop services."
echo " "
echo "Press Ctrl-C to abort or Enter to continue..."
read


# Docker network
if [ ! "$(docker network ls -q -f name=$NETWORK_NAME)" ]; then
    echo "Docker network '$NETWORK_NAME' does not exist, creating..."
    docker network create $NETWORK_NAME
fi

# (Try)cloudflare tunnel
if [ "$(docker ps -q -f name=$CLOUDFLARED_CONTAINER)" ]; then
    echo "Stopping and removing running container $CLOUDFLARED_CONTAINER"
    docker stop $CLOUDFLARED_CONTAINER
    docker rm $CLOUDFLARED_CONTAINER
elif [ "$(docker ps -aq -f status=exited -f name=$CLOUDFLARED_CONTAINER)" ]; then
    echo "Removing exited container $CLOUDFLARED_CONTAINER"
    docker rm $CLOUDFLARED_CONTAINER
fi

echo "Starting new container $CLOUDFLARED_CONTAINER"
docker run -d --name $CLOUDFLARED_CONTAINER --network $NETWORK_NAME $CLOUDFLARED_DOCKER_IMAGE tunnel --no-autoupdate --url http://clapshot_host:80
sleep 5


# Find out the public URL
echo " "
echo "Cloudflared logs:"
echo "================="
docker logs $CLOUDFLARED_CONTAINER
echo "================="
echo " "
CLOUDFLARED_URL=$(docker logs $CLOUDFLARED_CONTAINER 2>&1 | grep -o 'https://[a-zA-Z0-9.-]*\.trycloudflare\.com')
if [ -z "$CLOUDFLARED_URL" ]; then
  echo "ERROR: Cloudflared URL not found"
  exit 1
else
    if [[ ! "$CLOUDFLARED_URL" =~ ^https://[a-zA-Z0-9.-]*\.trycloudflare\.com$ ]]; then
        echo "ERROR: Invalid Cloudflared URL format: $CLOUDFLARED_URL"
        exit 1
    fi
    echo "Ok. Cloudflared URL: $CLOUDFLARED_URL"
fi


# Local data directory
if [ -d "$LOCAL_DATA_DIR" ]; then
    echo "Directory '$LOCAL_DATA_DIR' already exists."
else
    echo "Creating '$LOCAL_DATA_DIR' directory"
    mkdir -p "$LOCAL_DATA_DIR"
fi


# Clapshot host container
if [ "$(docker ps -q -f name=$CLAPSHOT_HOST_CONTAINER)" ]; then
    echo "Stopping and removing running container $CLAPSHOT_HOST_CONTAINER"
    docker stop $CLAPSHOT_HOST_CONTAINER
    docker rm $CLAPSHOT_HOST_CONTAINER
elif [ "$(docker ps -aq -f status=exited -f name=$CLAPSHOT_HOST_CONTAINER)" ]; then
    echo "Removing exited container $CLAPSHOT_HOST_CONTAINER"
    docker rm $CLAPSHOT_HOST_CONTAINER
fi
echo "Starting new container $CLAPSHOT_HOST_CONTAINER"
docker run -d --name $CLAPSHOT_HOST_CONTAINER --mount type=bind,source=$LOCAL_DATA_DIR,target=/mnt/clapshot-data/data --network $NETWORK_NAME -e CLAPSHOT_URL_BASE="$CLOUDFLARED_URL" $CLAPSHOT_DOCKER_IMAGE

# Start tailing the logs
sleep 3
docker logs -f $CLAPSHOT_HOST_CONTAINER

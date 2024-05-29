#!/bin/bash
set -e

# Local directory to bind mount to clapshot_host container. Modify if needed.
LOCAL_DATA_DIR="$(pwd)/CLAPSHOT_CLOUDFLARE_VOLUME"

# Set these if you have a Cloudflare tunnel token and don't want to use an anonymous (trycloudflare.com) tunnel:

#CLOUDFLARE_TUNNEL_TOKEN_FILE="$HOME/.cloudflared/clapshot-tunnel-token"
#CUSTOM_CLOUDFLARE_URL="https://demo.clapshot.io"


# (These you probably don't need to change)

# Docker images
CLAPSHOT_DOCKER_IMAGE="elonen/clapshot:latest-demo-htadmin"
CLOUDFLARED_DOCKER_IMAGE="cloudflare/cloudflared:latest"

# Network and container names
NETWORK_NAME="clapshot_cloudflare"
CLOUDFLARED_CONTAINER="cloudflared_for_clapshot"
CLAPSHOT_HOST_CONTAINER="clapshot_host"



# Checks and warnings

if [ -n "$CLOUDFLARE_TUNNEL_TOKEN_FILE" ] && [ -z "$CUSTOM_CLOUDFLARE_URL" ]; then
    echo "ERROR: CLOUDFLARE_TUNNEL_TOKEN_FILE is set but CUSTOM_CLOUDFLARE_URL is not."
    echo "This means you have a Cloudflare tunnel token but you are trying to use an anonymous (trycloudflare.com) tunnel."
    exit 1
elif [ -z "$CLOUDFLARE_TUNNEL_TOKEN_FILE" ] && [ -n "$CUSTOM_CLOUDFLARE_URL" ]; then
    echo "ERROR: CUSTOM_CLOUDFLARE_URL is set but CLOUDFLARE_TUNNEL_TOKEN_FILE is not."
    echo "This means you are trying to use a custom Cloudflare URL but you don't have a Cloudflare tunnel token."
    exit 1
fi

case "$(uname -sr)" in
     CYGWIN*|MINGW*|MINGW32*|MSYS*)
         echo "COMPATIBILITY WARNING: Running on a 'Unix lite' Windows shell. Bind mount on NTFS might cause issue with SQLite, symlinks etc. Consider WSL2 instead."
         echo " "
         ;;
esac

echo "--- SECURITY WARNING ---"
echo "This will expose your local dir '$LOCAL_DATA_DIR' to the Internet via "
echo -n "Clapshot server and Cloudflare tunnel "
if [ -n "$CUSTOM_CLOUDFLARE_URL" ]; then
    echo "on your custom URL: '$CUSTOM_CLOUDFLARE_URL'."
else
    echo "using an anonymous (trycloudflare.com) Cloudflare tunnel."
fi
echo " "
echo "It will start containers IN THE BACKGROUND."
echo "Simply closing this terminal will NOT stop the containers. You must use Docker commands to stop services."
echo " "
echo "Press Ctrl-C to abort or Enter to continue..."
read


# Local data dir
if [ -d "$LOCAL_DATA_DIR" ]; then
    echo "Ok, directory '$LOCAL_DATA_DIR' already exists."
else
    echo "Creating '$LOCAL_DATA_DIR' directory"
    mkdir -p "$LOCAL_DATA_DIR"
fi

# Docker network to connect the containers
if [ ! "$(docker network ls -q -f name=$NETWORK_NAME)" ]; then
    echo "Docker network '$NETWORK_NAME' does not exist, creating..."
    docker network create $NETWORK_NAME
fi

# Read tunnel token, if available
TOKEN_OPT=""
if [ -f "$CLOUDFLARE_TUNNEL_TOKEN_FILE" ]; then
    echo "Found Cloudflare tunnel token in '$CLOUDFLARE_TUNNEL_TOKEN_FILE', using it for authentication."
    CLOUDFLARE_TUNNEL_TOKEN=$(cat "$CLOUDFLARE_TUNNEL_TOKEN_FILE")
    TOKEN_OPT="--token $CLOUDFLARE_TUNNEL_TOKEN"
else
    echo "No tunnel token found in '$CLOUDFLARE_TUNNEL_TOKEN_FILE' => starting anonymous Cloudflare tunnel."
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

echo "Starting new container '$CLOUDFLARED_CONTAINER'"

if [ -z "$TOKEN_OPT" ]; then
    echo "WARNING: Cloudflare tunnel is anonymous. You may experience rate limiting."
    docker run -d --name $CLOUDFLARED_CONTAINER --network $NETWORK_NAME $CLOUDFLARED_DOCKER_IMAGE tunnel --no-autoupdate --url http://$CLAPSHOT_HOST_CONTAINER:80
else
    docker run -d --name $CLOUDFLARED_CONTAINER --network $NETWORK_NAME $CLOUDFLARED_DOCKER_IMAGE tunnel --no-autoupdate run $TOKEN_OPT --url http://$CLAPSHOT_HOST_CONTAINER:80
fi
sleep 5


# Find out the public URL
echo " "
echo "Cloudflared logs:"
echo "================="
docker logs $CLOUDFLARED_CONTAINER
echo "================="
echo " "

if [ -n "$CUSTOM_CLOUDFLARE_URL" ]; then
    echo "Using your your custom Cloudflare URL: $CUSTOM_CLOUDFLARE_URL"
    CLOUDFLARED_URL="$CUSTOM_CLOUDFLARE_URL"
else
    echo "Trying to find the (dynamic/anonymous) Cloudflare URL..."
    CLOUDFLARED_URL=$(docker logs $CLOUDFLARED_CONTAINER 2>&1 | grep -o 'https://[a-zA-Z0-9.-]*\.trycloudflare\.com')
    if [ -z "$CLOUDFLARED_URL" ]; then
    echo "ERROR: Cloudflared URL not found"
    exit 1
    else
        if [[ ! "$CLOUDFLARED_URL" =~ ^https://[a-zA-Z0-9.-]*\.trycloudflare\.com$ ]]; then
            echo "ERROR: Invalid Cloudflared URL format: $CLOUDFLARED_URL"
            exit 1
        fi
        echo "Ok. Dynamic Cloudflared URL found: $CLOUDFLARED_URL"
    fi
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

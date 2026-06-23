#!/bin/bash
set -e

# Docker named volume for Clapshot's data (DB, videos, gRPC sockets). Modify if needed.
# A named volume (not a host bind mount) keeps everything on the Linux VM's
# filesystem, so it works the same on Windows/macOS/Linux. Host bind mounts on
# Windows (NTFS via Docker's file-sharing layer) can't host the UNIX sockets
# Clapshot uses for server<->organizer gRPC, and break SQLite locking.
DATA_VOLUME="clapshot_cloudflare_data"

# Set these if you have a Cloudflare tunnel token and don't want to use an anonymous (trycloudflare.com) tunnel:

#CLOUDFLARE_TUNNEL_TOKEN_FILE="$HOME/.cloudflared/clapshot-tunnel-token"
#CUSTOM_CLOUDFLARE_URL="https://demo.clapshot.io"


# (These you probably don't need to change)

# Docker images
CLAPSHOT_DOCKER_IMAGE="elonen/clapshot:latest-demo-htwicket"
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
echo "This will expose Clapshot (data stored in Docker volume '$DATA_VOLUME') to the Internet via "
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


# Docker named volume for the data
if docker volume inspect "$DATA_VOLUME" >/dev/null 2>&1; then
    echo "Ok, Docker volume '$DATA_VOLUME' already exists."
else
    echo "Creating Docker volume '$DATA_VOLUME'..."
    docker volume create "$DATA_VOLUME"
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
# Find out the public URL
echo " "
if [ -n "$CUSTOM_CLOUDFLARE_URL" ]; then
    echo "Using your custom Cloudflare URL: $CUSTOM_CLOUDFLARE_URL"
    CLOUDFLARED_URL="$CUSTOM_CLOUDFLARE_URL"
else
    # The anonymous quick-tunnel URL takes ~5s to appear in the logs, so poll for
    # it instead of relying on a fixed sleep (that was a race we kept losing, which
    # made the script exit before starting Clapshot -> tunnel hit a dead origin).
    echo "Waiting for the (dynamic/anonymous) Cloudflare URL to appear in '$CLOUDFLARED_CONTAINER' logs..."
    CLOUDFLARED_URL=""
    for _ in $(seq 1 60); do
        if [ -z "$(docker ps -q -f name=$CLOUDFLARED_CONTAINER)" ]; then
            echo "ERROR: container '$CLOUDFLARED_CONTAINER' stopped before a URL appeared. Logs:"
            docker logs $CLOUDFLARED_CONTAINER 2>&1 | tail -30
            exit 1
        fi
        CLOUDFLARED_URL=$(docker logs $CLOUDFLARED_CONTAINER 2>&1 | grep -o 'https://[a-zA-Z0-9.-]*\.trycloudflare\.com' | head -1)
        [ -n "$CLOUDFLARED_URL" ] && break
        sleep 1
    done
    if [ -z "$CLOUDFLARED_URL" ]; then
        echo "ERROR: Cloudflared URL not found after 60s. Recent logs:"
        docker logs $CLOUDFLARED_CONTAINER 2>&1 | tail -30
        exit 1
    fi
    if [[ ! "$CLOUDFLARED_URL" =~ ^https://[a-zA-Z0-9.-]*\.trycloudflare\.com$ ]]; then
        echo "ERROR: Invalid Cloudflared URL format: $CLOUDFLARED_URL"
        exit 1
    fi
    echo "Ok. Dynamic Cloudflared URL found: $CLOUDFLARED_URL"
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

# Build environment variable arguments for all CLAPSHOT_SERVER__ variables
ENV_ARGS=""
for var in $(env | grep '^CLAPSHOT_SERVER__' | cut -d= -f1); do
    ENV_ARGS="$ENV_ARGS -e $var=${!var}"
done

# Always set URL_BASE and CORS to Cloudflare URL (override any existing values)
ENV_ARGS="$ENV_ARGS -e CLAPSHOT_SERVER__URL_BASE=$CLOUDFLARED_URL -e CLAPSHOT_SERVER__CORS=$CLOUDFLARED_URL"

docker run -d --name $CLAPSHOT_HOST_CONTAINER --mount type=volume,source=$DATA_VOLUME,target=/mnt/clapshot-data/data --network $NETWORK_NAME $ENV_ARGS $CLAPSHOT_DOCKER_IMAGE


# Wait until Clapshot's nginx actually serves, so the public URL doesn't greet
# visitors with a transient "Bad gateway" while the server is still booting.
echo "Waiting for Clapshot to come up..."
for _ in $(seq 1 30); do
    if [ -z "$(docker ps -q -f name=$CLAPSHOT_HOST_CONTAINER)" ]; then
        echo "ERROR: container '$CLAPSHOT_HOST_CONTAINER' exited during startup. Logs:"
        docker logs $CLAPSHOT_HOST_CONTAINER 2>&1 | tail -40
        exit 1
    fi
    code=$(docker exec $CLAPSHOT_HOST_CONTAINER curl -s -o /dev/null -w '%{http_code}' http://localhost:80/ 2>/dev/null || true)
    [ -n "$code" ] && [ "$code" != "000" ] && break
    sleep 1
done

echo " "
echo "==================================================================="
echo "  Clapshot is now available at:  $CLOUDFLARED_URL"
echo "==================================================================="
echo " "

# Start tailing the logs
docker logs -f $CLAPSHOT_HOST_CONTAINER

/// <reference types="svelte" />
/// <reference types="vite/client" />

declare global {
    namespace NodeJS {
        interface ProcessEnv {
            CLAPSHOT_CLIENT_VERSION: string;
            CLAPSHOT_MIN_SERVER_VERSION: string;
            CLAPSHOT_MAX_SERVER_VERSION: string;
        }
    }
}

export {};

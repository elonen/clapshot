// This emulates expirable cookies using local storage.

type BackendCookie = {
    value: string;
    expiration: number;
};

class LocalStorageCookies
{
    private static STORAGE_KEY = "clapshot_state";

    // Sets the cookie value and expiration timestamp.
    // If value is empty, the cookie is deleted.
    public static set(key: string, value: string, expirationTimestamp: number|null): void {
        if (!expirationTimestamp) {
            expirationTimestamp = new Date().getTime() + 60 * 60 * 12; // 12 hours
        }
        const cookies = this.getCookies();
        if (key) {
            cookies[key] = { value: value, expiration: expirationTimestamp };
        } else {
            delete cookies[key];
        }
        localStorage.setItem(this.STORAGE_KEY, JSON.stringify(cookies));
    }

    // Returns the cookie value if it exists and is not expired, otherwise null.
    public static get(key: string): string | null {
        const cookies = this.getCookies();
        const cookie = cookies[key];
        if (!cookie) { return null; }
        const now = new Date().getTime();
        if (cookie.expiration <= now) {
            delete cookies[key];
            localStorage.setItem(this.STORAGE_KEY, JSON.stringify(cookies));
            return null;
        }
        return cookie.value;
    }

    // Returns all non-expired cookies in a dictionary.
    public static getAllNonExpired(): Record<string, string> {
        const cookies = this.getCookies();
        const currentTime = new Date().getTime();
        const res: Record<string, string> = {};
        for (const key in cookies) {
            const cookie = cookies[key];
            if (cookie.expiration > currentTime) {
                res[key] = cookie.value;
            } else {
                delete cookies[key];
            }
        }
        localStorage.setItem(this.STORAGE_KEY, JSON.stringify(cookies));
        return res;
    }

    private static getCookies(): Record<string, BackendCookie> {
        const cookiesJSON = localStorage.getItem(this.STORAGE_KEY);
        return cookiesJSON ? JSON.parse(cookiesJSON) : {};
    }
}

export default LocalStorageCookies;

/*
 * Спайк: нативный C-hook для лимита инструкций + scoped safe-pcall.
 *
 * 1) Hook. `luaL_error` вызывается из чистого C-фрейма, поэтому развёртывание
 *    (SEH на MSVC) не пересекает Rust-фрейм и не упирается в panic="abort".
 *    Перед ошибкой выставляется флаг g_limit_hit.
 * 2) Safe-pcall. Пока гвард взведён, глобальные `pcall`/`xpcall` заменяются на
 *    C-версии: они ловят ошибку как обычно, но если выставлен g_limit_hit —
 *    пробрасывают её дальше (lua_error из C-фрейма), не давая плагину проглотить
 *    лимит. Оригиналы сохраняются в реестре и возвращаются при снятии.
 *
 * Состояние — глобальное C (одно Lua-состояние на процесс). Для нескольких
 * инстансов счётчик/флаг нужно привязать к lua_State (P3/P5).
 *
 * Заголовки Lua не подключаем: объявляем нужный минимум вручную.
 * `lua_insert`/`lua_remove`/`lua_pop` — макросы, поэтому используем реальные
 * `lua_rotate`/`lua_settop`.
 */

typedef struct lua_State lua_State;
typedef struct lua_Debug lua_Debug;
typedef long long lua_KContext;
typedef int (*lua_KFunction)(lua_State *L, int status, lua_KContext ctx);
typedef int (*lua_CFunction)(lua_State *L);

extern void lua_sethook(lua_State *L, void (*f)(lua_State *, lua_Debug *), int mask, int count);
extern int luaL_error(lua_State *L, const char *fmt, ...);
extern int lua_error(lua_State *L);
extern int lua_pcallk(lua_State *L, int nargs, int nresults, int errfunc, lua_KContext ctx, lua_KFunction k);
extern void lua_pushcclosure(lua_State *L, lua_CFunction fn, int n);
extern void lua_pushboolean(lua_State *L, int b);
extern void lua_pushvalue(lua_State *L, int idx);
extern void lua_rotate(lua_State *L, int idx, int n);
extern void lua_settop(lua_State *L, int idx);
extern int lua_gettop(lua_State *L);
extern int lua_getfield(lua_State *L, int idx, const char *k);
extern void lua_setfield(lua_State *L, int idx, const char *k);
extern int lua_rawgeti(lua_State *L, int idx, long long n);
extern void lua_rawsetp(lua_State *L, int idx, const void *p);
extern int lua_rawgetp(lua_State *L, int idx, const void *p);

/* В Lua 5.5 REGISTRYINDEX = -(INT_MAX/2 + 1000), а не старое -1001000. */
#define LUA_REGISTRYINDEX (-(2147483647 / 2 + 1000))
#define LUA_RIDX_GLOBALS 2
#define LUA_MULTRET (-1)
#define LUA_OK 0
#define LUA_MASKCOUNT 8

#define lua_insert(L, idx) lua_rotate((L), (idx), 1)
#define lua_pop(L, n) lua_settop((L), -(n) - 1)

/* --- hook --- */

static unsigned long long g_limit = 0;
static unsigned long long g_count = 0;
static int g_step = 0;
static int g_limit_hit = 0;

static void proto_hook(lua_State *L, lua_Debug *ar) {
    (void)ar;
    g_count += (unsigned long long)g_step;
    if (g_limit != 0 && g_count > g_limit) {
        g_limit_hit = 1;
        luaL_error(L, "instruction_limit_exceeded");
    }
}

void proto_set_limit(unsigned long long limit) {
    g_limit = limit;
    g_count = 0;
    g_limit_hit = 0;
}

void proto_install_hook(lua_State *L, int step) {
    g_step = step;
    g_count = 0;
    g_limit_hit = 0;
    lua_sethook(L, proto_hook, LUA_MASKCOUNT, step);
}

void proto_remove_hook(lua_State *L) {
    lua_sethook(L, 0, 0, 0);
}

/* --- scoped safe pcall/xpcall --- */

static char K_PCALL;
static char K_XPCALL;
static int g_safe_installed = 0;

/* Вызывает сохранённый оригинал (pcall или xpcall) с теми же аргументами,
 * затем, если сработал лимит, пробрасывает ошибку из C-фрейма. Так плагин
 * получает точную семантику pcall/xpcall для обычных ошибок, но не может
 * проглотить лимит. */
static int proto_guarded_call(lua_State *L, void *key) {
    int n = lua_gettop(L);
    lua_rawgetp(L, LUA_REGISTRYINDEX, key); /* [orig, f, args...] */
    lua_insert(L, 1);
    int status = lua_pcallk(L, n, LUA_MULTRET, 0, 0, 0);
    if (status != LUA_OK) {
        if (g_limit_hit) {
            lua_error(L);
        }
        lua_pushboolean(L, 0);
        lua_insert(L, 1);
        return lua_gettop(L);
    }
    if (g_limit_hit) {
        int top = lua_gettop(L); /* результат: false, err */
        lua_pushvalue(L, top);   /* err */
        lua_error(L);            /* не даём плагину проглотить лимит */
    }
    return lua_gettop(L);
}

static int proto_safe_pcall(lua_State *L) {
    return proto_guarded_call(L, &K_PCALL);
}

static int proto_safe_xpcall(lua_State *L) {
    return proto_guarded_call(L, &K_XPCALL);
}

static void save_global(lua_State *L, const char *name, void *key) {
    lua_rawgeti(L, LUA_REGISTRYINDEX, LUA_RIDX_GLOBALS); /* [g] */
    lua_getfield(L, -1, name);                           /* [g, v] */
    lua_rotate(L, -2, 1);                                /* [v, g] */
    lua_settop(L, -2);                                   /* [v] */
    lua_rawsetp(L, LUA_REGISTRYINDEX, key);              /* [] */
}

static void restore_global(lua_State *L, const char *name, void *key) {
    lua_rawgetp(L, LUA_REGISTRYINDEX, key);              /* [v] */
    lua_rawgeti(L, LUA_REGISTRYINDEX, LUA_RIDX_GLOBALS); /* [v, g] */
    lua_rotate(L, -2, 1);                                /* [g, v] */
    lua_setfield(L, -2, name);                           /* [g] */
    lua_pop(L, 1);                                       /* [] */
}

void proto_install_safe_pcall(lua_State *L) {
    if (g_safe_installed) {
        return;
    }
    save_global(L, "pcall", &K_PCALL);
    save_global(L, "xpcall", &K_XPCALL);

    lua_rawgeti(L, LUA_REGISTRYINDEX, LUA_RIDX_GLOBALS);
    lua_pushcclosure(L, proto_safe_pcall, 0);
    lua_setfield(L, -2, "pcall");
    lua_pushcclosure(L, proto_safe_xpcall, 0);
    lua_setfield(L, -2, "xpcall");
    lua_pop(L, 1);

    g_safe_installed = 1;
}

void proto_restore_safe_pcall(lua_State *L) {
    if (!g_safe_installed) {
        return;
    }
    restore_global(L, "pcall", &K_PCALL);
    restore_global(L, "xpcall", &K_XPCALL);
    g_safe_installed = 0;
    g_limit_hit = 0;
}

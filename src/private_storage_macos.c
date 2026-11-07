/* macOS SDK constants/types are resolved by the native compiler. An explicit
 * empty, non-inheriting ACL is supplied atomically with creation: there is no
 * inherited-ACL window in which another account could retain an open handle.
 */
#include <stddef.h>
#include <sys/types.h>
#include <sys/acl.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <errno.h>

static filesec_t private_security(mode_t mode) {
    filesec_t security = filesec_init();
    if (security == NULL) return NULL;
    acl_t acl = acl_init(0);
    if (acl == NULL) {
        int saved = errno;
        filesec_free(security);
        errno = saved;
        return NULL;
    }
    acl_flagset_t flags;
    if (acl_get_flagset_np(acl, &flags) != 0 ||
        acl_add_flag_np(flags, ACL_FLAG_NO_INHERIT) != 0 ||
        filesec_set_property(security, FILESEC_MODE, &mode) != 0 ||
        filesec_set_property(security, FILESEC_ACL, &acl) != 0) {
        int saved = errno;
        acl_free(acl);
        filesec_free(security);
        errno = saved;
        return NULL;
    }
    acl_free(acl);
    return security;
}

int dpt_private_open(const char *path) {
    filesec_t security = private_security(0600);
    if (security == NULL) return -1;
    int result = openx_np(path, O_CREAT | O_EXCL | O_RDWR | O_NOFOLLOW | O_CLOEXEC,
                          security);
    int saved = errno;
    filesec_free(security);
    errno = saved;
    return result;
}

int dpt_private_mkdir(const char *path) {
    filesec_t security = private_security(0700);
    if (security == NULL) return -1;
    int result = mkdirx_np(path, security);
    int saved = errno;
    filesec_free(security);
    errno = saved;
    return result;
}

/* NULL with success means a verified absent ACL, never a failed stat/open. */
int dpt_read_acl(int fd, acl_t *out) {
    filesec_t security = filesec_init();
    struct stat metadata;
    if (security == NULL) return -1;
    *out = NULL;
    int result = fstatx_np(fd, &metadata, security);
    if (result == 0) {
        int present = 0;
        result = filesec_query_property(security, FILESEC_ACL, &present);
        if (result == 0 && present) {
            result = filesec_get_property(security, FILESEC_ACL, out);
            if (result == 0 && *out == NULL) { errno = EINVAL; result = -1; }
        }
    }
    int saved = errno;
    filesec_free(security);
    errno = saved;
    return result;
}

int dpt_acl_has_allow(acl_t acl) {
    if (acl_valid(acl) != 0) return -1;
    acl_entry_t entry;
    int selector = ACL_FIRST_ENTRY;
    while (acl_get_entry(acl, selector, &entry) == 0) {
        acl_tag_t tag;
        if (acl_get_tag_type(entry, &tag) != 0) return -1;
        if (tag == ACL_EXTENDED_ALLOW) return 1;
        if (tag != ACL_EXTENDED_DENY) { errno = EINVAL; return -1; }
        selector = ACL_NEXT_ENTRY;
    }
    /* Apple's acl_get_entry returns EINVAL at the end of a valid ACL. */
    return errno == EINVAL ? 0 : -1;
}

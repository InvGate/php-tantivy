#define FFI_SCOPE "TANTIVYPHP"
#define FFI_LIB "libtantivyphp.so"

char*   tv_version(void);
void    tv_string_free(char* s);
char*   tv_last_error(void);

unsigned long long tv_index_open_or_create(const char* config_json);
unsigned long long tv_index_open_readonly(const char* config_json);
int     tv_index_close(unsigned long long handle);

int     tv_add_document(unsigned long long handle, const char* doc_json);
int     tv_update_document(unsigned long long handle, const char* key_field, const char* key_value, const char* doc_json);
int     tv_delete_document(unsigned long long handle, const char* key_field, const char* key_value);
int     tv_commit(unsigned long long handle);
int     tv_optimize(unsigned long long handle);
long long tv_doc_count(unsigned long long handle);
char*   tv_search(unsigned long long handle, const char* query_json);

#include <weechat/weechat-plugin.h>
#include <stdlib.h>

void wdc_print(struct t_gui_buffer *buffer, const char *message);
void wdc_print_main(const char* message);
const char* wdc_config_get_plugin(const char* message);
int wdc_config_set_plugin(const char* message, const char* value);
struct t_gui_buffer *wdc_buffer_search(const char *name);
struct t_gui_buffer *wdc_buffer_new(const char *name, const char *data);
void wdc_buffer_set(struct t_gui_buffer *buffer, const char *property, const char *value);
void wdc_hook_signal_send(const char *signal, const char *type_data, void *signal_data);

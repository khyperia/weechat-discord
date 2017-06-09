#include <weechat-plugin.h>

// no idea why this exploded in the past
#ifndef NULL
#define NULL ((void*)0)
#endif

void
wdr_init(void);
void
wdr_end(void);

WEECHAT_PLUGIN_NAME("weecord");
WEECHAT_PLUGIN_DESCRIPTION("Discord support for weechat");
WEECHAT_PLUGIN_AUTHOR("khyperia <khyperia@live.com>");
WEECHAT_PLUGIN_VERSION("0.1");
WEECHAT_PLUGIN_LICENSE("GPL3");

static struct t_weechat_plugin* weechat_plugin;

int
weechat_plugin_init(struct t_weechat_plugin* plugin, int argc, char* argv[])
{
  (void)argc;
  (void)argv;
  weechat_plugin = plugin;
  wdr_init();
  return WEECHAT_RC_OK;
}

int
weechat_plugin_end(struct t_weechat_plugin* plugin)
{
  (void)plugin;
  wdr_end();
  return WEECHAT_RC_OK;
}

struct t_hook*
wdc_hook_command(const char* command,
                 const char* description,
                 const char* args,
                 const char* args_description,
                 const char* completion,
                 const void* pointer,
                 int (*callback)(const void* pointer,
                                 void* data,
                                 struct t_gui_buffer* buffer,
                                 int argc,
                                 char** argv,
                                 char** argv_eol))
{
  return weechat_hook_command(command,
                              description,
                              args,
                              args_description,
                              completion,
                              callback,
                              pointer,
                              NULL);
}

void
wdc_print(struct t_gui_buffer* buffer, const char* message)
{
  weechat_printf(buffer, "%s", message);
}

void
wdc_print_tags(struct t_gui_buffer* buffer,
               const char* tags,
               const char* message)
{
  weechat_printf_date_tags(buffer, 0, tags, "%s", message);
}

const char*
wdc_config_get_plugin(const char* message)
{
  return weechat_config_get_plugin(message);
}

int
wdc_config_set_plugin(const char* message, const char* value)
{
  switch (weechat_config_set_plugin(message, value)) {
    case WEECHAT_CONFIG_OPTION_SET_OK_CHANGED:
      return 0;
    case WEECHAT_CONFIG_OPTION_SET_OK_SAME_VALUE:
      return 1;
    case WEECHAT_CONFIG_OPTION_SET_OPTION_NOT_FOUND:
      return 2;
    case WEECHAT_CONFIG_OPTION_SET_ERROR:
    default:
      return 3;
  }
}

struct t_gui_buffer*
wdc_buffer_search(const char* name)
{
  return weechat_buffer_search("weecord", name);
}

struct t_gui_buffer*
wdc_buffer_new(const char* name,
               const void* pointer,
               int (*input_callback)(const void* pointer,
                                     void* data,
                                     struct t_gui_buffer* buffer,
                                     const char* input_data),
               int (*close_callback)(const void* pointer,
                                     void* data,
                                     struct t_gui_buffer* buffer))
{
  return weechat_buffer_new(
    name, input_callback, pointer, NULL, close_callback, pointer, NULL);
}

void
wdc_buffer_set(struct t_gui_buffer* buffer,
               const char* property,
               const char* value)
{
  weechat_buffer_set(buffer, property, value);
}

void
wdc_hook_signal_send(const char* signal,
                     const char* type_data,
                     void* signal_data)
{
  weechat_hook_signal_send(signal, type_data, signal_data);
}

void
wdc_load_backlog(void* buffer)
{
  wdc_hook_signal_send("logger_backlog", WEECHAT_HOOK_SIGNAL_POINTER, buffer);
}

struct t_hook*
wdc_hook_fd(int fd,
            const void* pointer,
            int (*callback)(const void* pointer, void* data, int fd))
{
  return weechat_hook_fd(fd, 1, 0, 0, callback, pointer, NULL);
}

void
wdc_unhook(struct t_hook* hook)
{
  weechat_unhook(hook);
}

int
wdc_nicklist_nick_exists(struct t_gui_buffer* buffer, const char* nick)
{
  struct t_gui_nick* gnick = weechat_nicklist_search_nick(buffer, NULL, nick);
  return gnick != NULL;
}

void
wdc_nicklist_add_nick(struct t_gui_buffer* buffer, const char* nick)
{
  const char* color = weechat_info_get("nick_color", nick);
  (void)weechat_nicklist_add_nick(buffer, NULL, nick, color, "", "", 1);
}

void
wdc_nicklist_remove_nick(struct t_gui_buffer* buffer, const char* nick)
{
  struct t_gui_nick* gnick = weechat_nicklist_search_nick(buffer, NULL, nick);
  if (gnick)
    weechat_nicklist_remove_nick(buffer, gnick);
}

const char*
wdc_info_get(const char* info_name, const char* arguments)
{
  return weechat_info_get(info_name, arguments);
}

void*
wdc_hdata_get(const char* name)
{
  return weechat_hdata_get(name);
}

const void*
wdc_hdata_get_var_hdata(void* hdata, const char* name)
{
  return weechat_hdata_get_var_hdata(hdata, name);
}

const char*
wdc_hdata_get_var_type_string(void* hdata, const char* name)
{
  return weechat_hdata_get_var_type_string(hdata, name);
}

int
wdc_hdata_integer(void* hdata, void* data, const char* name)
{
  return weechat_hdata_integer(hdata, data, name);
}

void*
wdc_hdata_pointer(void* hdata, void* obj, const char* name)
{
  return weechat_hdata_pointer(hdata, obj, name);
}

const char*
wdc_hdata_string(void* hdata, void* data, const char* name)
{
  return weechat_hdata_string(hdata, data, name);
}

void*
wdc_hook_completion(const char* completion_item,
                    const char* description,
                    const void* callback_pointer,
                    int (*callback)(const void*,
                                    void*,
                                    const char*,
                                    struct t_gui_buffer*,
                                    struct t_gui_completion*))
{
  return weechat_hook_completion(
    completion_item, description, callback, callback_pointer, NULL);
}

void
wdc_hook_completion_add(void* t_gui_completion, const char* word)
{
  weechat_hook_completion_list_add(
    (struct t_gui_completion*)t_gui_completion, word, 0, WEECHAT_LIST_POS_SORT);
}

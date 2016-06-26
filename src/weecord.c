#include <stdlib.h>
#include <string.h>
#include <weechat-plugin.h>

void wdr_init(void);
void wdr_end(void);
void wdr_command(void*, const char*);
void wdr_input(void*, const char*, const char*);
void wdr_hook_fd_callback(const void*, int);

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

static int
hook_command_callback(const void* pointer, void* data,
                      struct t_gui_buffer* buffer, int argc, char** argv,
                      char** argv_eol)
{
  if (argc < 2) {
    wdr_command(buffer, "");
  } else {
    wdr_command(buffer, argv_eol[1]);
  }
  return WEECHAT_RC_OK;
}

void
wdc_hook_command(const char* command, const char* description, const char* args,
                 const char* args_description, const char* completion)
{
  (void)weechat_hook_command(command, description, args, args_description,
                             completion, hook_command_callback, NULL, NULL);
}

void
wdc_print(struct t_gui_buffer* buffer, const char* message)
{
  weechat_printf(buffer, "%s", message);
}

void
wdc_print_tags(struct t_gui_buffer* buffer, const char* tags,
               const char* message)
{
  weechat_printf_tags(buffer, tags, "%s", message);
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

int
buffer_input_callback(const void* pointer, void* datatmp,
                      struct t_gui_buffer* buffer, const char* input_data)
{
  const char* data = (const char*)datatmp;
  wdr_input(buffer, data, input_data);
  return WEECHAT_RC_OK;
}

int
buffer_close_callback(const void* pointer, void* data,
                      struct t_gui_buffer* buffer)
{
  return WEECHAT_RC_OK;
}

struct t_gui_buffer*
wdc_buffer_new(const char* name, const char* data)
{
  // strdup result auto-freed by weechat on buffer close
  return weechat_buffer_new(name, buffer_input_callback, NULL, strdup(data),
                            buffer_close_callback, NULL, NULL);
}

void
wdc_buffer_set(struct t_gui_buffer* buffer, const char* property,
               const char* value)
{
  weechat_buffer_set(buffer, property, value);
}

void
wdc_hook_signal_send(const char* signal, const char* type_data,
                     void* signal_data)
{
  weechat_hook_signal_send(signal, type_data, signal_data);
}

void
wdc_load_backlog(void* buffer)
{
  wdc_hook_signal_send("logger_backlog", WEECHAT_HOOK_SIGNAL_POINTER, buffer);
}

static int
hook_fd_callback(const void* pointer, void* data, int fd)
{
  wdr_hook_fd_callback(pointer, fd);
  return WEECHAT_RC_OK;
}

void*
wdc_hook_fd(int fd, const void* pointer)
{
  return weechat_hook_fd(fd, 1, 0, 0, hook_fd_callback, pointer, NULL);
}

void
wdc_unhook(struct t_hook* hook)
{
  weechat_unhook(hook);
}

void
wdc_nicklist_add_nick(struct t_gui_buffer* buffer, const char* nick)
{
  struct t_gui_nick_group* grp =
    weechat_nicklist_search_group(buffer, NULL, "root_group");
  if (!grp) {
    grp = weechat_nicklist_add_group(buffer, NULL, "root_group",
                                     "weechat.color.nicklist_group", 1);
  }
  const char* color = weechat_info_get("nick_color", nick);
  (void)weechat_nicklist_add_nick(buffer, grp, nick, color, "", "", 1);
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

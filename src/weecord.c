#include <stdlib.h>
#include <string.h>
#include <weechat-plugin.h>

WEECHAT_PLUGIN_NAME("weecord");
WEECHAT_PLUGIN_DESCRIPTION("Discord support for weechat");
WEECHAT_PLUGIN_AUTHOR("khyperia <khyperia@live.com>");
WEECHAT_PLUGIN_VERSION("1.0");
WEECHAT_PLUGIN_LICENSE("GPL3");

static struct t_weechat_plugin *weechat_plugin;

static int discord_cmd_callback(const void *pointer, void *data, struct t_gui_buffer *buffer,
                                int argc, char **argv, char **argv_eol)
{
  if (argc < 2)
    {
      wdr_command(buffer, NULL);
    }
  else
    {
      wdr_command(buffer, argv_eol[1]);
    }
  return WEECHAT_RC_OK;
}

int weechat_plugin_init(struct t_weechat_plugin *plugin, int argc, char *argv[])
{
  (void)argc;
  (void)argv;
  weechat_plugin = plugin;
  weechat_hook_command("discord", "Control weechat-discord", "", "", "",
                       discord_cmd_callback, NULL, NULL);
  wdr_init();
  return WEECHAT_RC_OK;
}

int weechat_plugin_end(struct t_weechat_plugin *plugin)
{
  (void)plugin;
  wdr_end();
  return WEECHAT_RC_OK;
}
void wdc_print_main(const char* message)
{
    struct t_gui_buffer *buffer = weechat_buffer_search_main();
    wdc_print(buffer, message);
}

void wdc_print_tags(struct t_gui_buffer *buffer, const char *tags, const char *message)
{
    weechat_printf_tags(buffer, tags, "%s", message);
}

const char* wdc_config_get_plugin(const char* message)
{
    return weechat_config_get_plugin(message);
}

int wdr_config_set_plugin(const char* message, const char* value)
{
    switch (weechat_config_set_plugin (message, value))
    {
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

struct t_gui_buffer *wdc_buffer_search(const char *name)
{
    return weechat_buffer_search("weecord", name);
}

int buffer_input_callback(const void *pointer, void *datatmp, struct t_gui_buffer *buffer, const char *input_data)
{
    const char *data = (const char*)datatmp;
    wdr_input(buffer, data, input_data);
    return WEECHAT_RC_OK;
}

int buffer_close_callback(const void *pointer, void *data, struct t_gui_buffer *buffer)
{
    return WEECHAT_RC_OK;
}

struct t_gui_buffer *wdc_buffer_new(const char *name, const char *data)
{
    // strdup result auto-freed by weechat on buffer close
    return weechat_buffer_new(name, buffer_input_callback, NULL, strdup(data), buffer_close_callback, NULL, NULL);
}

void wdc_buffer_set(struct t_gui_buffer *buffer, const char *property, const char *value)
{
    weechat_buffer_set(buffer, property, value);
}

void wdc_hook_signal_send(const char *signal, const char *type_data, void *signal_data)
{
    weechat_hook_signal_send(signal, type_data, signal_data);
}

void wdc_nicklist_add_nick(struct t_gui_buffer *buffer, const char *nick)
{
    struct t_gui_nick_group *grp = weechat_nicklist_search_group(buffer, NULL, "root_group");
    if (!grp)
    {
        grp = weechat_nicklist_add_group(buffer, NULL, "root_group",
            "weechat.color.nicklist_group", 1);
    }
    const char *color = weechat_info_get("nick_color", nick);
    (void)weechat_nicklist_add_nick(buffer, grp, nick, color, "", "", 1);
}

const char *wdc_info_get(const char *info_name, const char *arguments)
{
    return weechat_info_get(info_name, arguments);
}

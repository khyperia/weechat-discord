#include "symbols.h"
#include <string.h>

// wdg = Weechat Discord Go
// wdc = Weechat Discord C

WEECHAT_PLUGIN_NAME("weechat-discord");
WEECHAT_PLUGIN_DESCRIPTION("Discord support for weechat");
WEECHAT_PLUGIN_AUTHOR("khyperia <khyperia@live.com>");
WEECHAT_PLUGIN_VERSION("1.0");
WEECHAT_PLUGIN_LICENSE("GPL3");

static struct t_weechat_plugin *weechat_plugin;

void wdg_init();
void wdg_end();
void wdg_command(struct t_gui_buffer *buffer, char *cmd);
void wdg_input(struct t_gui_buffer *buffer, const char *data, const char *input_data);

static int discord_cmd_callback(const void *pointer, void *data, struct t_gui_buffer *buffer,
                                int argc, char **argv, char **argv_eol)
{
    if (argc < 2)
    {
        wdg_command(buffer, NULL);
    }
    else
    {
        wdg_command(buffer, argv_eol[1]);
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
    wdg_init();
    return WEECHAT_RC_OK;
}

int weechat_plugin_end(struct t_weechat_plugin *plugin)
{
    (void)plugin;
    wdg_end();
    return WEECHAT_RC_OK;
}

void wdc_print(struct t_gui_buffer *buffer, const char *message)
{
    weechat_printf(buffer, message);
}

void wdc_print_main(const char* message)
{
    struct t_gui_buffer *buffer = weechat_buffer_search_main();
    wdc_print(buffer, message);
}

const char* wdc_config_get_plugin(const char* message)
{
    return weechat_config_get_plugin(message);
}

struct t_gui_buffer *wdc_buffer_search(const char *name)
{
    return weechat_buffer_search("weechat-discord", name);
}

int buffer_input_callback(const void *pointer, void *datatmp, struct t_gui_buffer *buffer, const char *input_data)
{
    const char *data = (const char*)datatmp;
    wdg_input(buffer, data, input_data);
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

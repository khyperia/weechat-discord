package main

/*
#include "symbols.h"
*/
import "C"
import (
	"unsafe"
	"github.com/bwmarrin/discordgo"
	"fmt"
	"strings"
)

const (
	// note: config_* is the name of the setting, not the actual setting value
	config_email = "email"
	config_password = "password"
	connect_cmd = "connect"
	email_cmd = "email "
	password_cmd = "password "
	crash_cmd = "crash"
)

var global_dg *discordgo.Session

func catch_print(ex interface{}, buffer *C.struct_t_gui_buffer) {
	if ex == nil {
		return
	}
	var msg string
	if err, ok := ex.(error); ok {
		msg = "[discord]\tUnhandled error: " + err.Error()
	} else {
		msg = "[discord]\tUnhandled panic non-error value: " + fmt.Sprint(ex)
	}
	if buffer != nil {
		print_buffer(buffer, msg)
	} else {
		print_main(msg)
	}
}

func print_buffer(buffer *C.struct_t_gui_buffer, message string) {
	str := C.CString(message)
	defer C.free(unsafe.Pointer(str))
	C.wdc_print(buffer, str)
}

func print_main(message string) {
	str := C.CString(message)
	defer C.free(unsafe.Pointer(str))
	C.wdc_print_main(str)
}

func config_get_plugin(option_name string) string {
	str := C.CString(option_name)
	defer C.free(unsafe.Pointer(str))
	ptr := C.wdc_config_get_plugin(str)
	if ptr == nil {
		return ""
	}
	return C.GoString(ptr)
}

func config_set_plugin(option_name string, value string) string {
	before := config_get_plugin(option_name)
	option := C.CString(option_name)
	defer C.free(unsafe.Pointer(option))
	val := C.CString(value)
	defer C.free(unsafe.Pointer(val))
	result := C.wdc_config_set_plugin(option, val)
	switch result {
	case 0:
		return "Option successfully changed from " + before + " to " + value
	case 1:
		if before == "" {
			return "Option set to " + value
		} else {
			return "Option already contained " + value
		}
	case 2:
		return "Option not found"
	//case 3:
	default:
		return "Error when setting that option"
	}
}

func buffer_search(name string) *C.struct_t_gui_buffer {
	str := C.CString(name)
	defer C.free(unsafe.Pointer(str))
	return C.wdc_buffer_search(str)
}

func buffer_new(name string, data string) *C.struct_t_gui_buffer {
	str := C.CString(name)
	defer C.free(unsafe.Pointer(str))
	dat := C.CString(data)
	defer C.free(unsafe.Pointer(dat))
	return C.wdc_buffer_new(str, dat)
}

func buffer_set(buffer *C.struct_t_gui_buffer, property string, value string) {
	prop := C.CString(property)
	defer C.free(unsafe.Pointer(prop))
	val := C.CString(value)
	defer C.free(unsafe.Pointer(val))
	C.wdc_buffer_set(buffer, prop, val)
}

func hook_signal_send(signal string, type_data string, signal_data unsafe.Pointer) {
	sig := C.CString(signal)
	defer C.free(unsafe.Pointer(sig))
	td := C.CString(type_data)
	defer C.free(unsafe.Pointer(td))
	C.wdc_hook_signal_send(sig, td, signal_data)
}

func load_backlog(buffer *C.struct_t_gui_buffer) {
	hook_signal_send("logger_backlog", C.WEECHAT_HOOK_SIGNAL_POINTER, unsafe.Pointer(buffer))
}

//export wdg_init
func wdg_init() {
	// ...
}

//export wdg_end
func wdg_end() {
	// ...
}

//export wdg_command
func wdg_command(buffer *C.struct_t_gui_buffer, params_c *C.char) {
	defer func() { catch_print(recover(), buffer) }()
	params := ""
	if params_c != nil {
		params = C.GoString(params_c)
	}
	const (
	)
	if params == connect_cmd {
		connect(buffer)
	} else if strings.HasPrefix(params, email_cmd) {
		params = params[len(email_cmd):]
		print_buffer(buffer, config_set_plugin(config_email, params))
	} else if strings.HasPrefix(params, password_cmd) {
		params = params[len(password_cmd):]
		print_buffer(buffer, config_set_plugin(config_password, params))
	} else if params == crash_cmd {
		intentional_crash()
	} else {
		print_buffer(buffer, "[discord]\tUnknown command: " + params)
	}
}

//export wdg_input
func wdg_input(buffer *C.struct_t_gui_buffer, data *C.char, input_data *C.char) {
	defer func() { catch_print(recover(), buffer) }()
	input(global_dg, buffer, C.GoString(data), C.GoString(input_data))
}

func connect(buffer *C.struct_t_gui_buffer) {
	email := config_get_plugin(config_email)
	password := config_get_plugin(config_password)
	if email == "" || password == "" {
		print_buffer(buffer, "Error: plugins.var.weechat-discord.{email,password} unset. Run:")
		if email == "" {
			print_buffer(buffer, "/discord email youremail@example.com")
		}
		if password == "" {
			print_buffer(buffer, "/discord password hunter2")
		}
		return
	}
	print_buffer(buffer, "Discord: Connecting");
	dg, err := discordgo.New(email, password)
	if err != nil {
		print_buffer(buffer, err.Error())
		return
	}

	add_handlers(dg)

	err = dg.Open()
	if err != nil {
		print_buffer(buffer, err.Error())
		return
	}

	print_buffer(buffer, "Discord: Connected")

	open_buffers(buffer, dg)

	global_dg = dg
}

func add_handlers(dg *discordgo.Session) {
	dg.AddHandler(messageCreate)
	dg.AddHandler(messageUpdate)
	dg.AddHandler(messageDelete)
	dg.AddHandler(channelCreate)
	dg.AddHandler(channelDelete)
}

func open_buffers(buffer *C.struct_t_gui_buffer, dg *discordgo.Session) {
	guilds, err := dg.UserGuilds()
	if err == nil {
		for _, guild := range guilds {
			channels, err := dg.GuildChannels(guild.ID)
			if err == nil {
				for _, channel := range channels {
					get_buffer(guild, channel)
				}
			} else {
				print_buffer(buffer, err.Error())
			}
		}
	} else {
		print_buffer(buffer, err.Error())
	}
	channels, err := dg.UserChannels()
	if err == nil {
		for _, channel := range channels {
			get_buffer_id(dg, channel.ID)
		}
	} else {
		print_buffer(buffer, err.Error())
	}
}

func intentional_crash() {
	var dg *discordgo.Session = nil
	print_main(fmt.Sprint(dg.Debug))
}

func get_buffer(server *discordgo.Guild, channel *discordgo.Channel) *C.struct_t_gui_buffer {
	if channel.Type != "text" {
		return nil
	}
	var server_id string
	var server_name string
	if server == nil {
		server_id = "0"
		server_name = "pm"
	} else {
		server_id = server.ID
		server_name = server.Name
	}
	channel_id := channel.ID
	var channel_name string
	if channel.Recipient == nil {
		channel_name = "#" + channel.Name
	} else {
		channel_name = channel.Recipient.Username
	}
	buffer_id := server_id + "." + channel_id
	buffer_name := server_name + " " + channel_name
	buffer := buffer_search(buffer_id)
	if buffer == nil {
		buffer = buffer_new(buffer_id, channel_id)
		buffer_set(buffer, "short_name", buffer_name)
		buffer_set(buffer, "title", channel.Topic)
		buffer_set(buffer, "type", "formatted")
		buffer_set(buffer, "nicklist", "1")
		load_backlog(buffer)
	}
	return buffer
}

func get_buffer_id(dg *discordgo.Session, channel_id string) *C.struct_t_gui_buffer {
	channel, err := dg.Channel(channel_id)
	if err != nil {
		panic(err)
	}
	if channel.Type != "text" {
		return nil
	}
	server, err := dg.Guild(channel.GuildID)
	if err != nil {
		server = nil
	}
	return get_buffer(server, channel)
}

func messageCreate(dg *discordgo.Session, m *discordgo.MessageCreate) {
	var buffer *C.struct_t_gui_buffer
	defer func() { catch_print(recover(), buffer) }()
	buffer = get_buffer_id(dg, m.ChannelID)
	if buffer == nil {
		return // TODO
	}
	print_buffer(buffer, m.Author.Username + "\t" + m.Content)
}

func messageUpdate(dg *discordgo.Session, m *discordgo.MessageUpdate) {
	var buffer *C.struct_t_gui_buffer
	defer func() { catch_print(recover(), buffer) }()
	buffer = get_buffer_id(dg, m.ChannelID)
	if buffer == nil {
		return // TODO
	}
	var author_name string
	if m.Author == nil {
		author_name = "[unknown]"
	} else {
		author_name = m.Author.Username
	}
	print_buffer(buffer, author_name + "\tEDIT: " + m.Content)
}

func messageDelete(dg *discordgo.Session, m *discordgo.MessageDelete) {
	var buffer *C.struct_t_gui_buffer
	defer func() { catch_print(recover(), buffer) }()
	buffer = get_buffer_id(dg, m.ChannelID)
	if buffer == nil {
		return // TODO
	}
	// currently really broken, just always displays "[unknown] DELETE: "
	var author_name string
	if m.Author == nil {
		author_name = "[unknown]"
	} else {
		author_name = m.Author.Username
	}
	print_buffer(buffer, author_name + "\tDELETE: " + m.Content)
}

func channelCreate(dg *discordgo.Session, m *discordgo.ChannelCreate) {
	defer func() { catch_print(recover(), nil) }()
	guild, _ := dg.Guild(m.GuildID)
	get_buffer(guild, m.Channel)
}

func channelDelete(dg *discordgo.Session, m *discordgo.ChannelDelete) {
	var buffer *C.struct_t_gui_buffer
	defer func() { catch_print(recover(), buffer) }()
	guild, _ := dg.Guild(m.GuildID)
	buffer = get_buffer(guild, m.Channel)
	if buffer == nil {
		return // TODO
	}
	print_buffer(buffer, "[discord]\tCHANNEL DELETED")
}

func input(dg *discordgo.Session, buffer *C.struct_t_gui_buffer, channel_id string, input_data string) {
	_, err := dg.ChannelMessageSend(channel_id, input_data)
	if err != nil {
		print_buffer(buffer, err.Error())
	}
}

func main() {
	// never used, just needed by cgo
}

package main

// TODO: Cachies

/*
#include "symbols.h"
*/
import "C"
import (
	"unsafe"
	"github.com/bwmarrin/discordgo"
	"fmt"
	"strings"
	"regexp"
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
var my_user_id string

func catch_print(ex interface{}) {
	if ex == nil {
		return
	}
	var msg string
	if err, ok := ex.(error); ok {
		msg = "[discord]\tUnhandled error: " + err.Error()
	} else {
		msg = "[discord]\tUnhandled panic non-error value: " + fmt.Sprint(ex)
	}
	print_main(msg)
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

func print_buffer_tags(buffer *C.struct_t_gui_buffer, tags string, message string) {
	tag := C.CString(tags)
	defer C.free(unsafe.Pointer(tag))
	msg := C.CString(message)
	defer C.free(unsafe.Pointer(msg))
	C.wdc_print_tags(buffer, tag, msg)
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

func nick_color(nick string) string {
	nick_color := C.CString("irc_nick_color")
	defer C.free(unsafe.Pointer(nick_color))
	nk := C.CString(nick)
	defer C.free(unsafe.Pointer(nk))
	ptr := C.wdc_info_get(nick_color, nk)
	if ptr == nil {
		return ""
	}
	return C.GoString(ptr)
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
	defer func() {
		catch_print(recover())
	}()
	params := ""
	if params_c != nil {
		params = C.GoString(params_c)
	}
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
	defer func() {
		catch_print(recover())
	}()
	input(global_dg, buffer, C.GoString(data), C.GoString(input_data))
}

func connect(buffer *C.struct_t_gui_buffer) {
	email := config_get_plugin(config_email)
	password := config_get_plugin(config_password)
	if email == "" || password == "" {
		print_buffer(buffer, "Error: plugins.var.weecord.{email,password} unset. Run:")
		if email == "" {
			print_buffer(buffer, "/discord email your.email@example.com")
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

	global_dg = dg
}

func add_handlers(dg *discordgo.Session) {
	dg.AddHandler(ready)
	dg.AddHandler(messageCreate)
	dg.AddHandler(messageUpdate)
	dg.AddHandler(messageDelete)
	dg.AddHandler(channelCreate)
	dg.AddHandler(channelDelete)
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
		if buffer == nil {
			print_main("Unable to create buffer")
			return nil
		}
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

func ready(dg *discordgo.Session, m *discordgo.Ready) {
	my_user_id = m.User.ID
	for _, guild := range m.Guilds {
		// guilds are "Unavailable Guild Objects"
		channels, err := dg.GuildChannels(guild.ID)
		if err == nil {
			for _, channel := range channels {
				get_buffer(guild, channel)
			}
		} else {
			print_main(err.Error())
		}
	}
	for _, channel := range m.PrivateChannels {
		get_buffer(nil, channel)
	}
}

func messageCreate(dg *discordgo.Session, m *discordgo.MessageCreate) {
	defer func() {
		catch_print(recover())
	}()
	display_message(dg, m.Message, "")
}

func messageUpdate(dg *discordgo.Session, m *discordgo.MessageUpdate) {
	defer func() {
		catch_print(recover())
	}()
	display_message(dg, m.Message, "EDIT: ")
}

func messageDelete(dg *discordgo.Session, m *discordgo.MessageDelete) {
	defer func() {
		catch_print(recover())
	}()
	display_message(dg, m.Message, "DELETE: ")
}

func channelCreate(dg *discordgo.Session, m *discordgo.ChannelCreate) {
	defer func() {
		catch_print(recover())
	}()
	guild, _ := dg.Guild(m.GuildID)
	get_buffer(guild, m.Channel)
}

func channelDelete(dg *discordgo.Session, m *discordgo.ChannelDelete) {
	defer func() {
		catch_print(recover())
	}()
	guild, _ := dg.Guild(m.GuildID)
	buffer := get_buffer(guild, m.Channel)
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

var user_format_regex = regexp.MustCompile("<@\\d+>")
var nick_format_regex = regexp.MustCompile("<@!\\d+>")
var channel_format_regex = regexp.MustCompile("<#\\d+>")
var role_format_regex = regexp.MustCompile("<@&\\d+>")

func humanize_content(dg *discordgo.Session, message *discordgo.Message) string {
	content := message.Content
	content = user_format_regex.ReplaceAllStringFunc(content, func(match string) string {
		user, err := dg.User(match[2:len(match) - 1])
		print_main("match user " + match + " error lookup " + fmt.Sprint(err == nil))
		if err == nil {
			return "@" + user.Username
		} else {
			return match
		}
	})
	content = nick_format_regex.ReplaceAllStringFunc(content, func(match string) string {
		user, err := dg.User(match[3:len(match) - 1])
		print_main("match nick " + match + " error lookup " + fmt.Sprint(err == nil))
		if err == nil {
			return "@" + user.Username
		} else {
			return match
		}
	})
	content = channel_format_regex.ReplaceAllStringFunc(content, func(match string) string {
		user, err := dg.Channel(match[2:len(match) - 1])
		print_main("match channel " + match + " error lookup " + fmt.Sprint(err == nil))
		if err == nil {
			return "#" + user.Name
		} else {
			return match
		}
	})
	var roles []*discordgo.Role = nil
	content = role_format_regex.ReplaceAllStringFunc(content, func(match string) string {
		roleID := match[3:len(match) - 1]
		print_main("match role " + match)
		if roles == nil {
			channel, err := dg.Channel(message.ChannelID)
			if err != nil {
				return match
			}
			roles, err = dg.GuildRoles(channel.GuildID)
			if err != nil {
				return match
			}
		}
		for _, role := range roles {
			if role.ID == roleID {
				return role.Name
			}
		}
		return match
	})
	return content
}

/*
	ID              string        `json:"id"`
	ChannelID       string        `json:"channel_id"`
	Content         string        `json:"content"`
	Timestamp       string        `json:"timestamp"`
	EditedTimestamp string        `json:"edited_timestamp"`
	Tts             bool          `json:"tts"`
	MentionEveryone bool          `json:"mention_everyone"`
	Author          *User         `json:"author"`
	Attachments     []*Attachment `json:"attachments"`
	Embeds          []*Embed      `json:"embeds"`
	Mentions        []*User       `json:"mentions"`
*/
func display_message(dg *discordgo.Session, m *discordgo.Message, prefix string) {
	buffer := get_buffer_id(dg, m.ChannelID)
	if buffer == nil {
		return // TODO
	}
	content := humanize_content(dg, m)
	tags := make([]string, 0, 2)
	self_mentioned := m.MentionEveryone
	for _, mention := range m.Mentions {
		if self_mentioned {
			break
		}
		self_mentioned = self_mentioned || (mention.ID == my_user_id)
	}
	if self_mentioned {
		tags = append(tags, "notify_highlight")
	} else {
		// TODO: notify_private?
		tags = append(tags, "notify_message")
	}
	var author_name string
	if m.Author == nil {
		author_name = "[unknown]"
	} else {
		author_name = m.Author.Username
	}
	author_name = strings.Replace(author_name, ",", "", -1)
	tags = append(tags, "nick_" + author_name)
	color := nick_color(author_name)
	tags = append(tags, "prefix_nick_" + color)
	print_buffer_tags(buffer, strings.Join(tags, ","), author_name + "\t" + prefix + content)
}

func main() {
	// never used, just needed by cgo
}

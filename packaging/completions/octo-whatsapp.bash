# bash completion for octo-whatsapp (Phase 5 Part G).
# Install: source this file from your .bashrc, or copy to
# /etc/bash_completion.d/octo-whatsapp.

_octo_whatsapp() {
    local cur prev words cword
    _init_completion || return

    # Top-level commands.
    local commands="daemon mcp version status health send groups messages \
chats envelope media capabilities domain rules triggers audit actions \
events clients methods reconnect shutdown onboard"

    if [[ ${cword} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "${commands}" -- "${cur}") )
        return 0
    fi

    # Per-command subcommand completions.
    case "${words[1]}" in
        send)
            COMPREPLY=( $(compgen -W "text image video audio voice sticker reaction poll contact location delete" -- "${cur}") )
            ;;
        groups)
            COMPREPLY=( $(compgen -W "list info create leave members admins invite subject description announce locked ephemeral approval" -- "${cur}") )
            ;;
        messages)
            COMPREPLY=( $(compgen -W "list get search edit mark-read download" -- "${cur}") )
            ;;
        chats)
            COMPREPLY=( $(compgen -W "list info pin unpin mute archive delete typing" -- "${cur}") )
            ;;
        envelope)
            COMPREPLY=( $(compgen -W "encode decode send send-native" -- "${cur}") )
            ;;
        media)
            COMPREPLY=( $(compgen -W "info upload download" -- "${cur}") )
            ;;
        rules)
            COMPREPLY=( $(compgen -W "list get create update patch delete enable disable approve reload flush test" -- "${cur}") )
            ;;
        triggers)
            COMPREPLY=( $(compgen -W "list get create update delete run" -- "${cur}") )
            ;;
        audit)
            COMPREPLY=( $(compgen -W "tail verify" -- "${cur}") )
            ;;
        actions)
            COMPREPLY=( $(compgen -W "escalate" -- "${cur}") )
            ;;
        events)
            COMPREPLY=( $(compgen -W "list show tail replay" -- "${cur}") )
            ;;
        clients)
            COMPREPLY=( $(compgen -W "list" -- "${cur}") )
            ;;
        methods)
            COMPREPLY=( $(compgen -W "list help" -- "${cur}") )
            ;;
        domain)
            COMPREPLY=( $(compgen -W "compute-hash" -- "${cur}") )
            ;;
        onboard)
            COMPREPLY=( $(compgen -W "qr-link qr-code code-link" -- "${cur}") )
            ;;
    esac
    return 0
}

complete -F _octo_whatsapp octo-whatsapp
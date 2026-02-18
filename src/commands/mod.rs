use std::sync::Arc;

use chrono::{DateTime, FixedOffset};
use serenity::all::{
    ButtonStyle, CommandInteraction, ComponentInteraction, ComponentInteractionDataKind,
    CreateActionRow, CreateAttachment, CreateButton, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage, EditMessage, MessageId,
    ModalInteraction,
};
use tracing::warn;

use crate::{
    app_state::AppState,
    errors::InternalError,
    models::{Confession, ConfessionStatus},
};

macro_rules! check_permission {
    ($state:ident, $interaction:ident) => {
        let Some(guild_id) = $interaction.guild_id else {
            $interaction
                .create_response(
                    &$state.serenity_http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .ephemeral(true)
                            .content("This item must be used in a server."),
                    ),
                )
                .await?;
            return Ok(());
        };

        if !$interaction
            .user
            .has_role(
                &$state.serenity_http,
                guild_id,
                $state.config.confessions_role_id,
            )
            .await?
        {
            $interaction
                .create_response(
                    &$state.serenity_http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .ephemeral(true)
                            .content("You are not allowed to use this item."),
                    ),
                )
                .await?;
            return Ok(());
        }
    };
}

async fn get_confession_by_message_id(
    db: &mut sqlx::SqliteConnection,
    message_id: MessageId,
) -> Result<Option<Confession>, sqlx::Error> {
    let message_id_i64 = message_id.get() as i64;

    sqlx::query_as!(
        Confession,
        r#"SELECT
            id,
            message_id,
            reviewer_id,
            content,
            status "status: _",
            rejected_reason,
            created_at "created_at: _"
        FROM confessions WHERE message_id = ?"#,
        message_id_i64
    )
    .fetch_optional(db)
    .await
}

pub async fn handle_command(
    state: Arc<AppState>,
    interaction: &CommandInteraction,
) -> Result<(), InternalError> {
    // command for exporting approved confessions and put them in a text file for publishing
    // clears the queue afterwards, all approved posts are moved to posted and approval cannot be
    // undone anymore
    // rejected posts can still be un-rejected

    match interaction.data.name.as_str() {
        "confessions" => {
            let limit = interaction
                .data
                .options
                .iter()
                .find(|o| o.name == "limit")
                .map(|o| o.value.as_i64())
                .flatten()
                .unwrap_or(10);

            check_permission!(state, interaction);
            interaction.defer(&state.serenity_http).await?;

            let mut txn = state.pool.begin().await?;
            let approved_confessions = sqlx::query_as!(
                Confession,
                r#"SELECT
                    id,
                    message_id,
                    reviewer_id,
                    content,
                    status "status: _",
                    rejected_reason,
                    created_at "created_at: _"
                FROM confessions WHERE status = 'Approved'
                ORDER BY id
                LIMIT ?"#,
                limit
            )
            .fetch_all(&mut *txn)
            .await?;

            if approved_confessions.is_empty() {
                interaction
                    .create_followup(
                        &state.serenity_http,
                        CreateInteractionResponseFollowup::new()
                            .content("Chưa có confession nào được duyệt."),
                    )
                    .await?;
                txn.rollback().await?;

                return Ok(());
            }

            let mut post = String::with_capacity(
                approved_confessions
                    .iter()
                    .map(|c| c.content.len() + 100)
                    .sum::<usize>()
                    + 35
                    + state.config.confessions_submission_url.len(),
            );

            for confession in approved_confessions.iter() {
                post += &format!(
                    "#deadpage{} [{}]\n{}\n__________________________\n\n",
                    confession.id,
                    DateTime::<FixedOffset>::from_naive_utc_and_offset(
                        confession.created_at.naive_utc(),
                        FixedOffset::east_opt(7 * 3600).expect("can make fixed offset UTC+7")
                    )
                    .format("%H:%M:%S %d/%m/%Y"),
                    confession.content
                );
            }

            post += &format!(
                "chúc các bạn một ngày tốt lành òwó\n{}",
                state.config.confessions_submission_url
            );

            interaction
                .create_followup(
                    &state.serenity_http,
                    CreateInteractionResponseFollowup::new()
                        .add_file(CreateAttachment::bytes(post.as_bytes(), "confessions.txt")),
                )
                .await?;

            for confession in approved_confessions.iter() {
                if let Some(Ok(message_id)) = confession
                    .message_id
                    .map(|mid| MessageId::try_from(mid as u64))
                {
                    state
                        .serenity_http
                        .edit_message(
                            state.config.confessions_channel_id,
                            message_id,
                            &EditMessage::new().components(vec![CreateActionRow::Buttons(vec![
                                CreateButton::new("posted")
                                    .label("Đã tạo bài đăng")
                                    .disabled(true),
                            ])]),
                            vec![],
                        )
                        .await?;
                }

                sqlx::query!(
                    "UPDATE confessions SET status = 'Posted' WHERE id = ?",
                    confession.id
                )
                .execute(&mut *txn)
                .await?;
            }

            txn.commit().await?;
        }
        _ => {
            warn!(interaction = ?interaction, command = interaction.data.name, "received unknown command");
            interaction
                .create_response(
                    &state.serenity_http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .ephemeral(true)
                            .content("Unknown command"),
                    ),
                )
                .await?;
        }
    }

    Ok(())
}

pub async fn handle_component(
    state: Arc<AppState>,
    interaction: &mut ComponentInteraction,
) -> Result<(), InternalError> {
    let mut txn = state.pool.begin().await?;

    match interaction.data.kind {
        ComponentInteractionDataKind::Button
            if interaction.data.custom_id == "approve-confession"
                || interaction.data.custom_id == "reject-confession" =>
        {
            check_permission!(state, interaction);
            interaction.defer_ephemeral(&state.serenity_http).await?;

            let Some(confession) =
                get_confession_by_message_id(&mut *txn, interaction.message.id).await?
            else {
                warn!(message_id = ?interaction.message.id, "received approve-confession/reject-confession on unknown message");
                interaction
                    .create_followup(
                        &state.serenity_http,
                        CreateInteractionResponseFollowup::new().content(format!(
                            "Không tìm thấy confession từ ID tin nhắn {}",
                            interaction.message.id
                        )),
                    )
                    .await?;
                txn.rollback().await?;

                return Ok(());
            };

            let status = if interaction.data.custom_id == "approve-confession" {
                ConfessionStatus::Approved
            } else {
                ConfessionStatus::Rejected
            };

            let reviewer_id = interaction.user.id.get() as i64;

            sqlx::query!(
                "UPDATE confessions SET status = ?, reviewer_id = ? WHERE id = ?",
                status,
                reviewer_id,
                confession.id
            )
            .execute(&mut *txn)
            .await?;

            let message = if status == ConfessionStatus::Approved {
                format!("Đã duyệt confession #{}", confession.id)
            } else {
                format!("Đã từ chối confession #{}", confession.id)
            };

            interaction
                .create_followup(
                    &state.serenity_http,
                    CreateInteractionResponseFollowup::new().content(message),
                )
                .await?;

            let buttons = if status == ConfessionStatus::Approved {
                vec![
                    CreateButton::new("approved")
                        .style(ButtonStyle::Success)
                        .label(format!("Duyệt bởi {}", interaction.user.name))
                        .disabled(true),
                    CreateButton::new("reject-confession")
                        .style(ButtonStyle::Danger)
                        .label("Từ chối"),
                ]
            } else {
                vec![
                    CreateButton::new("approve-confession")
                        .style(ButtonStyle::Success)
                        .label("Duyệt"),
                    CreateButton::new("rejected")
                        .style(ButtonStyle::Danger)
                        .label(format!("Từ chối bởi {}", interaction.user.name))
                        .disabled(true),
                ]
            };

            interaction
                .message
                .edit(
                    &state.serenity_http,
                    EditMessage::new().components(vec![CreateActionRow::Buttons(buttons)]),
                )
                .await?;
        }
        _ => {
            warn!(kind = ?interaction.data.kind, custom_id = interaction.data.custom_id, "unknown component interaction");
        }
    }

    txn.commit().await?;

    Ok(())
}

pub async fn handle_modal(
    state: Arc<AppState>,
    interaction: &ModalInteraction,
) -> Result<(), InternalError> {
    check_permission!(state, interaction);

    // TODO: implement modal for rejection reason

    Ok(())
}

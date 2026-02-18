use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serenity::all::{
    ButtonStyle, ChannelId, CreateActionRow, CreateButton, CreateEmbed, CreateMessage, EditMessage,
    Message, MessageBuilder, MessageId,
};

use crate::{
    app_state::AppState,
    errors::InternalError,
    models::{Confession, ConfessionStatus},
};

#[derive(Deserialize)]
pub struct ConfessionCreate {
    pub index: i64,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

async fn edit_confession_message(
    serenity_http: &serenity::http::Http,
    confessions_channel_id: ChannelId,
    message_id: MessageId,
    data: &ConfessionCreate,
) -> Result<Message, serenity::Error> {
    serenity_http
        .edit_message(
            confessions_channel_id,
            message_id,
            &EditMessage::new()
                .embed(
                    CreateEmbed::new()
                        .title(format!("hàng nhái cfs #{}", data.index))
                        .description(MessageBuilder::new().push_safe(&data.content).build())
                        .timestamp(data.timestamp),
                )
                .components(vec![CreateActionRow::Buttons(vec![
                    CreateButton::new("approve-confession")
                        .style(ButtonStyle::Success)
                        .label("Duyệt"),
                    CreateButton::new("reject-confession")
                        .style(ButtonStyle::Danger)
                        .label("Từ chối"),
                ])]),
            vec![],
        )
        .await
}

async fn post_confession_message(
    serenity_http: &serenity::http::Http,
    db: &mut sqlx::SqliteConnection,
    confessions_channel_id: ChannelId,
    data: &ConfessionCreate,
    confession_id: Option<i64>,
) -> Result<Message, InternalError> {
    let message = confessions_channel_id
        .send_message(
            serenity_http,
            CreateMessage::new()
                .add_embed(
                    CreateEmbed::new()
                        .title(format!("hàng nhái cfs #{}", data.index))
                        .description(MessageBuilder::new().push_safe(&data.content).build())
                        .timestamp(data.timestamp),
                )
                .components(vec![CreateActionRow::Buttons(vec![
                    CreateButton::new("approve-confession")
                        .style(ButtonStyle::Success)
                        .label("Duyệt"),
                    CreateButton::new("reject-confession")
                        .style(ButtonStyle::Danger)
                        .label("Từ chối"),
                ])]),
        )
        .await?;

    if let Some(confession_id) = confession_id {
        let message_id_i64 = message.id.get() as i64;

        sqlx::query!(
            "UPDATE confessions SET message_id = ? WHERE id = ?",
            message_id_i64,
            confession_id
        )
        .execute(db)
        .await?;
    }

    Ok(message)
}

pub async fn post_confession(
    State(app_state): State<Arc<AppState>>,
    Json(data): Json<ConfessionCreate>,
) -> Result<Response, InternalError> {
    // TODO: security measures (probably a static auth key is good enough)
    // TODO: insert into database

    // Check if there's already a confession with the provided ID
    let mut txn = app_state.pool.begin().await?;
    let confession = sqlx::query_as!(
        Confession,
        r#"SELECT
            id,
            message_id,
            reviewer_id,
            content,
            status "status: _",
            rejected_reason,
            created_at "created_at: _"
        FROM confessions WHERE id = ?"#,
        data.index
    )
    .fetch_optional(&mut *txn)
    .await?;
    let timestamp = data.timestamp.timestamp();

    match confession {
        Some(confession) if confession.status != ConfessionStatus::Posted => {
            // If there is, and the confession is not already posted, update the confession and send it back to pending
            sqlx::query!(
                "UPDATE confessions SET content = ?, status = 'Pending', rejected_reason = NULL, created_at = ? WHERE id = ?",
                data.content,
                timestamp,
                confession.id
            )
            .execute(&mut *txn)
            .await?;

            // There's a chance that the confession was registered in the database but it never made it to Discord.
            if let Some(Ok(message_id)) = confession
                .message_id
                .map(|mid| MessageId::try_from(mid as u64))
            {
                edit_confession_message(
                    &app_state.serenity_http,
                    app_state.config.confessions_channel_id,
                    message_id,
                    &data,
                )
                .await?;
                app_state
                    .config
                    .confessions_channel_id
                    .send_message(
                        &app_state.serenity_http,
                        CreateMessage::new().content(format!(
                            "Confession đã được chỉnh sửa: https://discord.com/channels/{}/{}/{}. Trạng thái duyệt hoặc từ chối (nếu có) đã bị đặt lại.",
                            app_state.config.confessions_guild_id,
                            app_state.config.confessions_channel_id,
                            message_id
                        )),
                    )
                    .await?;
            } else {
                post_confession_message(
                    &app_state.serenity_http,
                    &mut *txn,
                    app_state.config.confessions_channel_id,
                    &data,
                    Some(confession.id),
                )
                .await?;
            }
        }
        Some(confession) => {
            // If there is, but the confession is already posted, update the confession's content but notify that it was changed
            sqlx::query!(
                "UPDATE confessions SET content = ?, created_at = ? WHERE id = ?",
                data.content,
                timestamp,
                confession.id
            )
            .execute(&mut *txn)
            .await?;

            app_state
                .config
                .confessions_channel_id
                .send_message(
                    &app_state.serenity_http,
                    CreateMessage::new()
                        .content(format!(
                            "Confession đã được chỉnh sửa sau khi đăng bài. Confession gốc: https://discord.com/channels/{}/{}/{}",
                            app_state.config.confessions_guild_id,
                            app_state.config.confessions_channel_id,
                            confession.message_id.unwrap_or(0), // unlikely that the confession was already approved but there's no attached message ID
                        ))
                        .add_embed(
                            CreateEmbed::new()
                                .title(format!("hàng nhái cfs #{}", data.index))
                                .description(MessageBuilder::new().push_safe(&data.content).build())
                                .timestamp(data.timestamp),
                        ),
                )
                .await?;
        }
        None => {
            // If there isn't, treat as new post
            let confession_id = sqlx::query!(
                "INSERT INTO confessions (id, content, created_at) VALUES (?, ?, ?) RETURNING id",
                data.index,
                data.content,
                timestamp
            )
            .fetch_one(&mut *txn)
            .await?;

            post_confession_message(
                &app_state.serenity_http,
                &mut *txn,
                app_state.config.confessions_channel_id,
                &data,
                Some(confession_id.id),
            )
            .await?;
        }
    }

    txn.commit().await?;

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}

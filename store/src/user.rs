use crate::models::user::User;
use crate::Store;
use uuid::Uuid;
use bcrypt::{hash, DEFAULT_COST};

#[derive(Debug)]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
    pub public_key: String,
}

#[derive(Debug)]
pub enum UserError {
    UserExists,
    InvalidInput(String),
    DatabaseError(String),
    PasswordHashingError(String),
}

impl std::fmt::Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserError::UserExists => write!(f, "User already exists"),
            UserError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            UserError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            UserError::PasswordHashingError(msg) => write!(f, "Password hashing error: {}", msg),
        }
    }
}

impl std::error::Error for UserError {}

impl Store {
    pub async fn create_user(&self, request: CreateUserRequest) -> Result<User, UserError> {
        if !request.email.contains('@') {
            return Err(UserError::InvalidInput("Invalid email format".to_string()));
        }

        if request.password.len() < 6 {
            return Err(UserError::InvalidInput(
                "Password must be at least 6 characters".to_string(),
            ));
        }

        let existing_user = self.get_user_by_email(&request.email).await?;

        if existing_user.is_some() {
            return Err(UserError::UserExists);
        }

        let password_hash = hash(&request.password, DEFAULT_COST)
            .map_err(|e| UserError::PasswordHashingError(e.to_string()))?;

        let user = sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (email, password_hash, public_key)
            VALUES ($1, $2, $3)
            RETURNING id, email, password_hash, public_key, created_at, updated_at
            "#,
            request.email,
            password_hash,
            request.public_key
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        Ok(user)
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, UserError> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, password_hash, public_key, created_at, updated_at
            FROM users
            WHERE email = $1
            "#,
            email
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        Ok(user)
    }

    pub async fn get_user_by_id(&self, user_id: Uuid) -> Result<Option<User>, UserError> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, password_hash, public_key, created_at, updated_at
            FROM users
            WHERE id = $1
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        Ok(user)
    }

    pub async fn get_user_by_public_key(&self, public_key: &str) -> Result<Option<User>, UserError> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, password_hash, public_key, created_at, updated_at
            FROM users
            WHERE public_key = $1
            "#,
            public_key
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| UserError::DatabaseError(e.to_string()))?;

        Ok(user)
    }
}

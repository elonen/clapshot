"""Add message table

Revision ID: 5b50ae9ea790
Revises: 35faeadd84c3
Create Date: 2022-09-22 12:47:27.906393

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '5b50ae9ea790'
down_revision = '35faeadd84c3'
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table('message',
        sa.Column('id', sa.Integer(), autoincrement=True, nullable=False),
        sa.Column('user_id', sa.String(), nullable=True),
        sa.Column('created', sa.DateTime(), server_default=sa.text('(CURRENT_TIMESTAMP)'), nullable=False),
        sa.Column('seen', sa.Boolean(), nullable=True),
        sa.Column('ref_video_hash', sa.Integer(), nullable=True),
        sa.Column('ref_comment_id', sa.Integer(), nullable=True),
        sa.Column('event_name', sa.String(), nullable=True),
        sa.Column('message', sa.String(), nullable=True),
        sa.Column('details', sa.String(), nullable=True),
        sa.ForeignKeyConstraint(['ref_comment_id'], ['comment.id'], ),
        sa.ForeignKeyConstraint(['ref_video_hash'], ['video.video_hash'], ),
        sa.PrimaryKeyConstraint('id'),
        sqlite_autoincrement=True
    )


def downgrade() -> None:
    op.drop_table('message')

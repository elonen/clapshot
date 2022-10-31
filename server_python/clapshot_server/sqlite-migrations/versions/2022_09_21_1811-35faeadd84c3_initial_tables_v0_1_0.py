"""Initial tables, v0.1.0

Revision ID: 35faeadd84c3
Revises: 
Create Date: 2022-09-21 18:11:45.675686

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '35faeadd84c3'
down_revision = None
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table('video',
        sa.Column('id', sa.Integer(), autoincrement=True, nullable=False),
        sa.Column('video_hash', sa.String(), nullable=True),
        sa.Column('added_by_userid', sa.String(), nullable=True),
        sa.Column('added_by_username', sa.String(), nullable=True),
        sa.Column('added_time', sa.DateTime(), server_default=sa.text('(CURRENT_TIMESTAMP)'), nullable=False),
        sa.Column('recompression_done', sa.DateTime(), nullable=True),
        sa.Column('orig_filename', sa.String(), nullable=True),
        sa.Column('total_frames', sa.Integer(), nullable=True),
        sa.Column('duration', sa.Float(), nullable=True),
        sa.Column('fps', sa.String(), nullable=True),
        sa.Column('raw_metadata_video', sa.String(), nullable=True),
        sa.Column('raw_metadata_all', sa.String(), nullable=True),
        sa.PrimaryKeyConstraint('id')
    )
    op.create_index(op.f('ix_video_added_by_userid'), 'video', ['added_by_userid'], unique=False)
    op.create_index(op.f('ix_video_video_hash'), 'video', ['video_hash'], unique=True)

    op.create_table('comment',
        sa.Column('id', sa.Integer(), autoincrement=True, nullable=False),
        sa.Column('video_hash', sa.Integer(), nullable=False),
        sa.Column('parent_id', sa.Integer(), nullable=True),
        sa.Column('created', sa.DateTime(), server_default=sa.text('(CURRENT_TIMESTAMP)'), nullable=False),
        sa.Column('edited', sa.DateTime(), nullable=True),
        sa.Column('user_id', sa.String(), nullable=True),
        sa.Column('username', sa.String(), nullable=True),
        sa.Column('comment', sa.String(), nullable=True),
        sa.Column('timecode', sa.String(), nullable=True),
        sa.Column('drawing', sa.String(), nullable=True),
        sa.ForeignKeyConstraint(['parent_id'], ['comment.id'], ),
        sa.ForeignKeyConstraint(['video_hash'], ['video.video_hash'], ),
        sa.PrimaryKeyConstraint('id'),
        sqlite_autoincrement=True
    )
    op.create_index(op.f('ix_comment_parent_id'), 'comment', ['parent_id'], unique=False)


def downgrade() -> None:
    op.drop_index(op.f('ix_comment_parent_id'), table_name='comment')
    op.drop_table('comment')
    op.drop_index(op.f('ix_video_video_hash'), table_name='video')
    op.drop_index(op.f('ix_video_added_by_userid'), table_name='video')
    op.drop_table('video')

"""add-not-nulls

Revision ID: f4dba652e0c2
Revises: 118e820845d5
Create Date: 2022-10-04 23:07:41.630747

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = 'f4dba652e0c2'
down_revision = '118e820845d5'
branch_labels = None
depends_on = None


def upgrade() -> None:

    with op.batch_alter_table("comment") as bop:
        bop.alter_column('user_id',
                existing_type=sa.VARCHAR(),
                nullable=False)
        bop.alter_column('username',
                existing_type=sa.VARCHAR(),
                nullable=False)
        bop.alter_column('comment',
                existing_type=sa.VARCHAR(),
                nullable=False)

    with op.batch_alter_table("message") as bop:
        bop.alter_column('user_id',
                existing_type=sa.VARCHAR(),
                nullable=False)
        bop.alter_column('seen',
                existing_type=sa.BOOLEAN(),
                nullable=False)
        bop.alter_column('event_name',
                existing_type=sa.VARCHAR(),
                nullable=False)
        bop.alter_column('message',
                existing_type=sa.VARCHAR(),
                nullable=False)
        bop.alter_column('details',
                existing_type=sa.VARCHAR(),
                nullable=False)

    with op.batch_alter_table("video") as bop:
        bop.alter_column('video_hash',
                existing_type=sa.VARCHAR(),
                nullable=False)
        bop.alter_column('orig_filename',
                existing_type=sa.VARCHAR(),
                nullable=False)
        bop.alter_column('total_frames',
                existing_type=sa.INTEGER(),
                nullable=False)
        bop.alter_column('duration',
                existing_type=sa.FLOAT(),
                nullable=False)
        bop.alter_column('fps',
                existing_type=sa.VARCHAR(),
                nullable=False)

def downgrade() -> None:

    with op.batch_alter_table("video") as bop:
        bop.alter_column('fps',
                existing_type=sa.VARCHAR(),
                nullable=True)
        bop.alter_column('duration',
                existing_type=sa.FLOAT(),
                nullable=True)
        bop.alter_column('total_frames',
                existing_type=sa.INTEGER(),
                nullable=True)
        bop.alter_column('orig_filename',
                existing_type=sa.VARCHAR(),
                nullable=True)
        bop.alter_column('video_hash',
                existing_type=sa.VARCHAR(),
                nullable=True)

    with op.batch_alter_table("message") as bop:
        bop.alter_column('details',
                existing_type=sa.VARCHAR(),
                nullable=True)
        bop.alter_column('message',
                existing_type=sa.VARCHAR(),
                nullable=True)
        bop.alter_column('event_name',
                existing_type=sa.VARCHAR(),
                nullable=True)
        bop.alter_column('seen',
                existing_type=sa.BOOLEAN(),
                nullable=True)
        bop.alter_column('user_id',
                existing_type=sa.VARCHAR(),
                nullable=True)

    with op.batch_alter_table("comment") as bop:
        bop.alter_column('comment',
                existing_type=sa.VARCHAR(),
                nullable=True)
        bop.alter_column('username',
                existing_type=sa.VARCHAR(),
                nullable=True)
        bop.alter_column('user_id',
                existing_type=sa.VARCHAR(),
                nullable=True)

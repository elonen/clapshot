from dataclasses import dataclass, asdict
from enum import Enum
from typing import Union


class DialogType(Enum):
    INPUT_LINE = 'input_line'
    YES_NO = 'yes_no'


@dataclass
class Dialog:
	type: DialogType
	title: str
	intro: str = None
	args: dict = None

	def dict(self):
		return asdict(self)


@dataclass
class Action:
	id: str
	api_cmd: str
	api_data: Union[str, dict, list] = None

	# OPTIONAL: show dialog for extra variables / confirmation
	dlg: Dialog = None
	exec_if: Union[str, bool] = True

	# OPTIONAL: visualize action in UI
	label: str = None
	key_shortcut: str = None
	icon_class: str = None

	def dict(self):
		return asdict(self)


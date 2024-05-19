import traceback

from grpclib import GRPCError
from grpclib.const import Status as GrpcStatus

import clapshot_grpc.proto.clapshot as clap
import clapshot_grpc.proto.clapshot.organizer as org
import sqlalchemy
import sqlalchemy.exc

from functools import wraps


def organizer_grpc_handler(func):
    """
    Decorator for gRPC handler methods in OrganizerInbound implementations.

    It displays a clean error message to the user through the Clapshot Client for some known errors,
    and handles some special cases of GRPCError that Clapshot server expects.

    In detail:

    - If a sqlalchemy.exc.OperationalError occurs, turn into a RESOURCE_EXHAUSTED error.
    - If a GRPCError occurs:
        - If it's ABORTED, pass it through -- it's a signal to Clapshot server to ignore the result.
        - If it's UNIMPLEMENTED, pass it through -- it's a signal to Clapshot server to decide what to do.
        - If it's INVALID_ARGUMENT, PERMISSION_DENIED, ALREADY_EXISTS or RESOURCE_EXHAUSTED, turned into a user message.
        - Otherwise, it's logged and passed through.
    """
    @wraps(func)
    async def wrapper(self, request):
        try:
            try:
                return await func(self, request)
            except sqlalchemy.exc.OperationalError as e:
                raise GRPCError(GrpcStatus.RESOURCE_EXHAUSTED, f"DB error: {e}")
        except GRPCError as e:
            # Pass some known errors through
            if e.status in (GrpcStatus.ABORTED, GrpcStatus.UNIMPLEMENTED):
                raise e
            # Intercept some known session errors and show them to the user nicely
            elif e.status in (GrpcStatus.INVALID_ARGUMENT, GrpcStatus.PERMISSION_DENIED, GrpcStatus.ALREADY_EXISTS, GrpcStatus.RESOURCE_EXHAUSTED):
                user_msg_was_sent = False
                try:
                    await self.srv.client_show_user_message(org.ClientShowUserMessageRequest(sid=request.ses.sid,
                        msg = clap.UserMessage(
                            message=str(e.message),
                            user_id=request.ses.user.id,
                            type=clap.UserMessageType.ERROR,
                            details=str(e.details) if e.details else None)))
                    user_msg_was_sent = True
                except Exception as e2:
                    self.log.error("Error calling client_show_user_message(): {e2}")
                if not user_msg_was_sent:
                    raise GRPCError(GrpcStatus.ABORTED)   # Tell Clapshot server to ignore the result (we've shown the error to the user)
            else:
                self.log.error(f"Unknown GRPCError in organizer_grpc_handler: {e}")
                raise e
        except Exception as e:
            self.log.error(f"General error in organizer_grpc_handler: {e}")
            self.log.error(traceback.format_exc())
            raise e

    return wrapper
